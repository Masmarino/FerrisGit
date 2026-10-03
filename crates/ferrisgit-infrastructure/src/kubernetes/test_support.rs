use k8s_openapi::api::core::v1::{Namespace, ServiceAccount};
use kube::api::{DeleteParams, PostParams};
use kube::config::{AuthInfo, KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};
use std::process::Command;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;

const CLUSTER_NAME: &str = "ferrisgit-test";
const CONTEXT_NAME: &str = "kind-ferrisgit-test";

static CLUSTER_READY: OnceLock<Mutex<bool>> = OnceLock::new();

/// Creates the shared kind cluster on first use and returns a client for it.
pub async fn test_client() -> Client {
    let lock = CLUSTER_READY.get_or_init(|| Mutex::new(false));
    let mut ready = lock.lock().await;
    if !*ready {
        let existing = Command::new("kind")
            .args(["get", "clusters"])
            .output()
            .expect("kind must be installed to run these tests");
        let names = String::from_utf8_lossy(&existing.stdout);
        if !names.lines().any(|line| line == CLUSTER_NAME) {
            let status = Command::new("kind")
                .args(["create", "cluster", "--name", CLUSTER_NAME])
                .status()
                .expect("failed to invoke kind create cluster");
            assert!(status.success(), "kind create cluster failed");
        }
        *ready = true;
    }
    drop(ready);

    let kubeconfig = Kubeconfig::read()
        .expect("no kubeconfig found; kind should have written one to ~/.kube/config");
    let options = KubeConfigOptions {
        context: Some(CONTEXT_NAME.to_string()),
        ..Default::default()
    };
    let config = Config::from_custom_kubeconfig(kubeconfig, &options)
        .await
        .expect("failed to build a Config for the kind-ferrisgit-test context");
    Client::try_from(config).expect("failed to build a kube::Client from the kind cluster config")
}

/// A throwaway namespace, deleted on drop. Fire-and-forget since Drop can't be async; a leak in a
/// disposable local cluster is harmless.
pub struct TestNamespace {
    pub name: String,
    client: Client,
}

impl TestNamespace {
    pub async fn create(client: &Client) -> Self {
        let name = format!("ferrisgit-test-{}", uuid::Uuid::new_v4());
        let namespaces: Api<Namespace> = Api::all(client.clone());
        let ns = Namespace {
            metadata: kube::api::ObjectMeta {
                name: Some(name.clone()),
                ..Default::default()
            },
            ..Default::default()
        };
        namespaces
            .create(&PostParams::default(), &ns)
            .await
            .expect("failed to create test namespace");
        wait_for_default_service_account(client, &name).await;
        Self {
            name,
            client: client.clone(),
        }
    }
}

/// Kubernetes creates the `default` ServiceAccount a moment after the namespace, and a pod created before
/// that is refused with a 403. Slow CI runners hit this window.
async fn wait_for_default_service_account(client: &Client, namespace: &str) {
    let accounts: Api<ServiceAccount> = Api::namespaced(client.clone(), namespace);
    for _ in 0..300 {
        if accounts
            .get_opt("default")
            .await
            .expect("failed to look up the default ServiceAccount")
            .is_some()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the default ServiceAccount of {namespace} did not appear within 30 s");
}

impl Drop for TestNamespace {
    fn drop(&mut self) {
        let client = self.client.clone();
        let name = self.name.clone();
        tokio::spawn(async move {
            let namespaces: Api<Namespace> = Api::all(client);
            let _ = namespaces.delete(&name, &DeleteParams::default()).await;
        });
    }
}

/// Client authenticated as the given ServiceAccount instead of cluster-admin, to check an RBAC role is
/// enough. The config is inferred again because `kube::Client` can't expose its own.
pub async fn scoped_client_for(namespace: &str, service_account: &str) -> Client {
    let output = TokioCommand::new("kubectl")
        .args([
            "--context",
            CONTEXT_NAME,
            "create",
            "token",
            service_account,
            "-n",
            namespace,
        ])
        .output()
        .await
        .expect("failed to invoke kubectl create token");
    assert!(
        output.status.success(),
        "kubectl create token failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let token = String::from_utf8(output.stdout).unwrap().trim().to_string();

    let options = KubeConfigOptions {
        context: Some(CONTEXT_NAME.to_string()),
        ..Default::default()
    };
    let kubeconfig = Kubeconfig::read().expect("no kubeconfig found");
    let mut config = Config::from_custom_kubeconfig(kubeconfig, &options)
        .await
        .expect("failed to build a base Config for the scoped client");
    config.default_namespace = namespace.to_string();
    config.auth_info = AuthInfo {
        token: Some(token.into()),
        ..Default::default()
    };
    Client::try_from(config).expect("failed to build a scoped kube::Client")
}

pub async fn kubectl_apply(namespace: &str, yaml: &str) {
    let mut child = TokioCommand::new("kubectl")
        .args([
            "--context",
            CONTEXT_NAME,
            "apply",
            "-n",
            namespace,
            "-f",
            "-",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to invoke kubectl apply");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(yaml.as_bytes())
        .await
        .expect("failed to write YAML to kubectl apply's stdin");
    let status = child.wait().await.expect("kubectl apply did not run");
    assert!(status.success(), "kubectl apply failed");
}
