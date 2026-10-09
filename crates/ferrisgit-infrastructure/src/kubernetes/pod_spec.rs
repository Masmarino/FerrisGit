use crate::kubernetes::cache::pvc_name;
use ferrisgit_domain::job::Job;
use ferrisgit_domain::job_containers::{
    CLONE_IMAGE, DOCKER_DAEMON_IMAGE, DOCKER_DIR, WORKSPACE_DIR, docker_daemon_command,
    docker_job_env, job_shell_script,
};
use k8s_openapi::api::core::v1::{
    Container, EmptyDirVolumeSource, EnvVar, EnvVarSource, ExecAction,
    PersistentVolumeClaimVolumeSource, Pod, PodSpec, Probe, Secret, SecretKeySelector,
    SecurityContext, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const JOB_ID_LABEL: &str = "ferrisgit.io/job-id";
const CACHE_MOUNT_ROOT: &str = "/ferrisgit-cache";

/// The key of the job's Secret that holds its clone token. A CI variable cannot have this name (no `-` in one).
pub const CLONE_TOKEN_KEY: &str = "ferrisgit-clone-token";

/// The container that runs the job's script, whose logs are the job's.
pub const JOB_CONTAINER: &str = "job";
/// The container that clones the repository first; its logs say why when it fails.
pub const CLONE_CONTAINER: &str = "clone";

const WORKSPACE_VOLUME: &str = "workspace";
const DOCKER_VOLUME: &str = "docker";
const DOCKER_DATA_VOLUME: &str = "docker-data";

/// Single place a Pod name is built, so `cancel`'s lookup can't drift from `build_pod`. The job's Secret has the same.
pub fn pod_name(job_id: Uuid) -> String {
    format!("ferrisgit-job-{job_id}")
}

/// Everything a job's Pod is made from.
pub struct PodJob<'a> {
    pub job: &'a Job,
    pub repository_id: Uuid,
    pub namespace: &'a str,
    /// Where to clone the repository from, without credentials: the token comes from the Secret.
    pub clone_url: &'a str,
    pub commit_sha: &'a str,
    /// The names of the CI variables, whose values are in the job's Secret.
    pub ci_variable_names: &'a [String],
    pub docker: bool,
}

fn env(name: &str, value: &str) -> EnvVar {
    EnvVar {
        name: name.to_string(),
        value: Some(value.to_string()),
        ..Default::default()
    }
}

fn env_from_secret(name: &str, secret: &str, key: &str) -> EnvVar {
    EnvVar {
        name: name.to_string(),
        value_from: Some(EnvVarSource {
            secret_key_ref: Some(SecretKeySelector {
                name: secret.to_string(),
                key: key.to_string(),
                optional: Some(false),
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn mount(volume: &str, path: &str) -> VolumeMount {
    VolumeMount {
        name: volume.to_string(),
        mount_path: path.to_string(),
        ..Default::default()
    }
}

fn empty_dir(name: &str) -> Volume {
    Volume {
        name: name.to_string(),
        empty_dir: Some(EmptyDirVolumeSource::default()),
        ..Default::default()
    }
}

/// A name a Secret key and an environment variable can both have.
fn is_secret_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Clones the commit into the workspace, then removes `.git`: the job gets the files, never the token. The token goes in
/// a header rather than the URL, so git never prints it.
const CLONE_SCRIPT: &str = r#"set -e
auth="$(printf 'job:%s' "$FERRISGIT_CLONE_TOKEN" | base64 | tr -d '\n')"
git -c http.extraHeader="Authorization: Basic $auth" clone --quiet "$FERRISGIT_CLONE_URL" /workspace
cd /workspace
git -c advice.detachedHead=false checkout --quiet "$FERRISGIT_COMMIT"
rm -rf .git"#;

fn clone_container(pod: &PodJob, secret: &str) -> Container {
    Container {
        name: CLONE_CONTAINER.to_string(),
        image: Some(CLONE_IMAGE.to_string()),
        command: Some(vec!["/bin/sh".to_string(), "-c".to_string()]),
        args: Some(vec![CLONE_SCRIPT.to_string()]),
        env: Some(vec![
            env("FERRISGIT_CLONE_URL", pod.clone_url),
            env("FERRISGIT_COMMIT", pod.commit_sha),
            env_from_secret("FERRISGIT_CLONE_TOKEN", secret, CLONE_TOKEN_KEY),
        ]),
        volume_mounts: Some(vec![mount(WORKSPACE_VOLUME, WORKSPACE_DIR)]),
        ..Default::default()
    }
}

/// The Docker daemon, as a native sidecar: it starts before the job, which waits until it answers, and stops with it.
/// It listens on a socket in a volume shared with the job only, never on the network, and copies its client there.
/// Privileged: Docker-in-Docker cannot run otherwise.
fn docker_container() -> Container {
    let socket = format!("unix://{DOCKER_DIR}/docker.sock");
    Container {
        name: "docker".to_string(),
        image: Some(DOCKER_DAEMON_IMAGE.to_string()),
        command: Some(vec!["/bin/sh".to_string(), "-c".to_string()]),
        args: Some(vec![docker_daemon_command()]),
        // No TLS certificates to make: nothing reaches the daemon but through the socket.
        env: Some(vec![env("DOCKER_TLS_CERTDIR", "")]),
        restart_policy: Some("Always".to_string()),
        security_context: Some(SecurityContext {
            privileged: Some(true),
            ..Default::default()
        }),
        startup_probe: Some(Probe {
            exec: Some(ExecAction {
                command: Some(vec![
                    "docker".to_string(),
                    format!("--host={socket}"),
                    "info".to_string(),
                ]),
            }),
            period_seconds: Some(1),
            failure_threshold: Some(120),
            ..Default::default()
        }),
        volume_mounts: Some(vec![
            mount(DOCKER_VOLUME, DOCKER_DIR),
            mount(DOCKER_DATA_VOLUME, "/var/lib/docker"),
        ]),
        ..Default::default()
    }
}

pub fn build_pod(pod: &PodJob) -> Pod {
    let job = pod.job;
    let name = pod_name(job.id);
    let mut labels = BTreeMap::new();
    labels.insert(JOB_ID_LABEL.to_string(), job.id.to_string());

    // The repository's CI variables come from the Secret and win over the file's variables of the same name.
    let ci_names: Vec<&String> = pod
        .ci_variable_names
        .iter()
        .filter(|key| is_secret_env_name(key))
        .collect();
    let mut env_vars: Vec<EnvVar> = job
        .variables
        .iter()
        .filter(|(key, _)| !ci_names.contains(key))
        .map(|(key, value)| env(key, value))
        .collect();
    env_vars.extend(ci_names.iter().map(|key| env_from_secret(key, &name, key)));
    if pod.docker {
        env_vars.extend(
            docker_job_env()
                .iter()
                .map(|(name, value)| env(name, value)),
        );
    }

    let mut volumes = vec![empty_dir(WORKSPACE_VOLUME)];
    let mut mounts = vec![mount(WORKSPACE_VOLUME, WORKSPACE_DIR)];
    for (i, key) in job.cache.iter().enumerate() {
        volumes.push(Volume {
            name: format!("cache-{i}"),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: pvc_name(pod.repository_id, key),
                read_only: Some(false),
            }),
            ..Default::default()
        });
        mounts.push(mount(
            &format!("cache-{i}"),
            &format!("{CACHE_MOUNT_ROOT}/{key}"),
        ));
    }

    let mut init_containers = vec![clone_container(pod, &name)];
    if pod.docker {
        volumes.push(empty_dir(DOCKER_VOLUME));
        volumes.push(empty_dir(DOCKER_DATA_VOLUME));
        mounts.push(mount(DOCKER_VOLUME, DOCKER_DIR));
        init_containers.push(docker_container());
    }

    Pod {
        metadata: ObjectMeta {
            name: Some(name),
            namespace: Some(pod.namespace.to_string()),
            labels: Some(labels),
            ..Default::default()
        },
        spec: Some(PodSpec {
            restart_policy: Some("Never".to_string()),
            init_containers: Some(init_containers),
            containers: vec![Container {
                name: JOB_CONTAINER.to_string(),
                image: Some(job.image.clone()),
                command: Some(vec!["/bin/sh".to_string(), "-c".to_string()]),
                args: Some(vec![job_shell_script(&job.script, pod.docker)]),
                working_dir: Some(WORKSPACE_DIR.to_string()),
                env: Some(env_vars),
                volume_mounts: Some(mounts),
                ..Default::default()
            }],
            volumes: Some(volumes),
            // The job's Pod has no business with the Kubernetes API.
            automount_service_account_token: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The job's Secret: its clone token and the repository's CI variables. It belongs to the Pod, so Kubernetes deletes it
/// with the Pod.
pub fn build_secret(
    job_id: Uuid,
    namespace: &str,
    pod_uid: &str,
    clone_token: &str,
    ci_variables: &BTreeMap<String, String>,
) -> Secret {
    let mut data: BTreeMap<String, String> = ci_variables
        .iter()
        .filter(|(key, _)| is_secret_env_name(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    data.insert(CLONE_TOKEN_KEY.to_string(), clone_token.to_string());
    let mut labels = BTreeMap::new();
    labels.insert(JOB_ID_LABEL.to_string(), job_id.to_string());
    Secret {
        metadata: ObjectMeta {
            name: Some(pod_name(job_id)),
            namespace: Some(namespace.to_string()),
            labels: Some(labels),
            owner_references: Some(vec![OwnerReference {
                api_version: "v1".to_string(),
                kind: "Pod".to_string(),
                name: pod_name(job_id),
                uid: pod_uid.to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        },
        string_data: Some(data),
        ..Default::default()
    }
}

/// A job's Pod with nothing to clone, for the tests that need a Pod to run and no server to clone from.
#[cfg(test)]
pub(crate) fn pod_without_clone(job: &Job, namespace: &str) -> Pod {
    let mut pod = build_pod(&PodJob {
        job,
        repository_id: Uuid::new_v4(),
        namespace,
        clone_url: "http://unused.test/none.git",
        commit_sha: "none",
        ci_variable_names: &[],
        docker: false,
    });
    pod.spec.as_mut().unwrap().init_containers = None;
    pod
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::cache::pvc_name;
    use chrono::Utc;
    use ferrisgit_domain::job::JobStatus;
    use std::collections::BTreeMap;

    fn fake_job(cache: Vec<String>) -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "rust:1.80".to_string(),
            script: vec!["cargo build".to_string(), "cargo test".to_string()],
            variables: BTreeMap::from([
                ("RUST_LOG".to_string(), "debug".to_string()),
                ("TOKEN".to_string(), "from-the-file".to_string()),
            ]),
            needs: vec![],
            tags: vec![],
            status: JobStatus::Pending,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            cache,
            started_at: None,
            finished_at: None,
        }
    }

    const CI_NAMES: [&str; 2] = ["TOKEN", "bad-name"];

    fn pod_of(job: &Job, repository_id: Uuid, docker: bool) -> Pod {
        let names: Vec<String> = CI_NAMES.iter().map(|n| n.to_string()).collect();
        build_pod(&PodJob {
            job,
            repository_id,
            namespace: "ferrisgit-jobs",
            clone_url: "http://ferrisgit:8080/acme/widget.git",
            commit_sha: "abc123",
            ci_variable_names: &names,
            docker,
        })
    }

    fn job_container(pod: &Pod) -> &Container {
        &pod.spec.as_ref().unwrap().containers[0]
    }

    fn init_names(pod: &Pod) -> Vec<String> {
        pod.spec
            .as_ref()
            .unwrap()
            .init_containers
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    #[test]
    fn the_pod_is_labeled_with_the_job_id_runs_in_its_namespace_and_never_restarts() {
        let job = fake_job(vec![]);
        let pod = pod_of(&job, Uuid::new_v4(), false);
        assert_eq!(
            pod.metadata.labels.as_ref().unwrap().get(JOB_ID_LABEL),
            Some(&job.id.to_string())
        );
        assert_eq!(pod.metadata.namespace.as_deref(), Some("ferrisgit-jobs"));
        let spec = pod.spec.as_ref().unwrap();
        assert_eq!(spec.restart_policy.as_deref(), Some("Never"));
        assert_eq!(spec.automount_service_account_token, Some(false));
    }

    #[test]
    fn the_repository_is_cloned_first_into_the_workspace_the_job_runs_from() {
        let job = fake_job(vec![]);
        let pod = pod_of(&job, Uuid::new_v4(), false);

        assert_eq!(init_names(&pod), ["clone"]);
        let clone = &pod.spec.as_ref().unwrap().init_containers.as_ref().unwrap()[0];
        assert_eq!(clone.image.as_deref(), Some(CLONE_IMAGE));
        let env = clone.env.as_ref().unwrap();
        assert!(env.iter().any(|e| e.name == "FERRISGIT_CLONE_URL"
            && e.value.as_deref() == Some("http://ferrisgit:8080/acme/widget.git")));
        assert!(
            env.iter()
                .any(|e| e.name == "FERRISGIT_COMMIT" && e.value.as_deref() == Some("abc123"))
        );
        let token = env
            .iter()
            .find(|e| e.name == "FERRISGIT_CLONE_TOKEN")
            .unwrap();
        assert!(
            token.value.is_none(),
            "the token never sits in the Pod's spec"
        );
        assert_eq!(
            token
                .value_from
                .as_ref()
                .unwrap()
                .secret_key_ref
                .as_ref()
                .unwrap()
                .key,
            CLONE_TOKEN_KEY
        );

        let container = job_container(&pod);
        assert_eq!(container.working_dir.as_deref(), Some("/workspace"));
        assert!(
            container
                .volume_mounts
                .as_ref()
                .unwrap()
                .iter()
                .any(|m| m.mount_path == "/workspace")
        );
    }

    #[test]
    fn the_container_uses_the_jobs_image_and_runs_every_script_line() {
        let job = fake_job(vec![]);
        let pod = pod_of(&job, Uuid::new_v4(), false);
        let container = job_container(&pod);
        assert_eq!(container.image.as_deref(), Some("rust:1.80"));
        assert_eq!(
            container.args.as_ref().unwrap()[0],
            "cargo build && cargo test"
        );
    }

    #[test]
    fn the_ci_variables_come_from_the_secret_and_win_over_the_files_variables() {
        let job = fake_job(vec![]);
        let pod = pod_of(&job, Uuid::new_v4(), false);
        let env = job_container(&pod).env.as_ref().unwrap();

        assert!(
            env.iter()
                .any(|e| e.name == "RUST_LOG" && e.value.as_deref() == Some("debug"))
        );
        let tokens: Vec<&EnvVar> = env.iter().filter(|e| e.name == "TOKEN").collect();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0]
                .value_from
                .as_ref()
                .unwrap()
                .secret_key_ref
                .as_ref()
                .unwrap()
                .name,
            pod_name(job.id)
        );
        assert!(
            !env.iter().any(|e| e.name == "bad-name"),
            "a name no shell can export is left out"
        );
        assert!(
            !env.iter().any(|e| e.name == "FERRISGIT_CLONE_TOKEN"),
            "the job never gets the clone token"
        );
    }

    #[test]
    fn with_docker_a_privileged_daemon_runs_beside_the_job_on_a_shared_socket() {
        let job = fake_job(vec![]);
        let pod = pod_of(&job, Uuid::new_v4(), true);

        assert_eq!(init_names(&pod), ["clone", "docker"]);
        let daemon = &pod.spec.as_ref().unwrap().init_containers.as_ref().unwrap()[1];
        assert_eq!(daemon.image.as_deref(), Some(DOCKER_DAEMON_IMAGE));
        assert_eq!(
            daemon.restart_policy.as_deref(),
            Some("Always"),
            "a native sidecar"
        );
        assert_eq!(
            daemon.security_context.as_ref().unwrap().privileged,
            Some(true)
        );
        assert!(
            daemon.startup_probe.is_some(),
            "the job waits until the daemon answers"
        );

        let container = job_container(&pod);
        assert!(
            container.security_context.is_none(),
            "the job itself is not privileged"
        );
        assert!(
            container
                .env
                .as_ref()
                .unwrap()
                .iter()
                .any(|e| e.name == "DOCKER_HOST"
                    && e.value.as_deref() == Some("unix:///ferrisgit-docker/docker.sock"))
        );
        assert!(
            container
                .env
                .as_ref()
                .unwrap()
                .iter()
                .any(|e| e.name == "DOCKER_CONFIG"
                    && e.value.as_deref() == Some("/ferrisgit-docker/config")),
            "the client finds buildx and compose"
        );
        assert!(
            container.args.as_ref().unwrap()[0]
                .starts_with("export PATH=\"$PATH:/ferrisgit-docker/bin\"")
        );
        assert!(
            container
                .volume_mounts
                .as_ref()
                .unwrap()
                .iter()
                .any(|m| m.mount_path == DOCKER_DIR)
        );
    }

    #[test]
    fn without_docker_there_is_no_daemon_and_no_privilege() {
        let job = fake_job(vec![]);
        let pod = pod_of(&job, Uuid::new_v4(), false);
        let spec = pod.spec.as_ref().unwrap();
        assert!(
            spec.init_containers
                .as_ref()
                .unwrap()
                .iter()
                .all(|c| c.security_context.is_none())
        );
        assert!(
            !job_container(&pod)
                .env
                .as_ref()
                .unwrap()
                .iter()
                .any(|e| e.name == "DOCKER_HOST")
        );
    }

    #[test]
    fn each_cache_key_mounts_its_own_pvc_at_a_distinct_path() {
        let repository_id = Uuid::new_v4();
        let job = fake_job(vec!["cargo-registry".to_string(), "target-dir".to_string()]);
        let pod = pod_of(&job, repository_id, false);
        let spec = pod.spec.as_ref().unwrap();
        let claim_names: Vec<String> = spec
            .volumes
            .as_ref()
            .unwrap()
            .iter()
            .filter_map(|v| {
                v.persistent_volume_claim
                    .as_ref()
                    .map(|c| c.claim_name.clone())
            })
            .collect();
        assert_eq!(
            claim_names,
            [
                pvc_name(repository_id, "cargo-registry"),
                pvc_name(repository_id, "target-dir")
            ]
        );
        let paths: Vec<&str> = spec.containers[0]
            .volume_mounts
            .as_ref()
            .unwrap()
            .iter()
            .map(|m| m.mount_path.as_str())
            .filter(|path| path.starts_with(CACHE_MOUNT_ROOT))
            .collect();
        assert_eq!(
            paths,
            [
                "/ferrisgit-cache/cargo-registry",
                "/ferrisgit-cache/target-dir"
            ]
        );
    }

    #[test]
    fn the_secret_holds_the_token_and_the_variables_and_belongs_to_the_pod() {
        let job_id = Uuid::new_v4();
        let variables = BTreeMap::from([
            ("DEPLOY_TOKEN".to_string(), "s3cret".to_string()),
            ("bad name".to_string(), "x".to_string()),
        ]);

        let secret = build_secret(job_id, "ns", "uid-1", "clone-token", &variables);

        let data = secret.string_data.unwrap();
        assert_eq!(data.get("DEPLOY_TOKEN").map(String::as_str), Some("s3cret"));
        assert_eq!(
            data.get(CLONE_TOKEN_KEY).map(String::as_str),
            Some("clone-token")
        );
        assert!(!data.contains_key("bad name"));
        let owner = &secret.metadata.owner_references.unwrap()[0];
        assert_eq!((owner.kind.as_str(), owner.uid.as_str()), ("Pod", "uid-1"));
        assert_eq!(secret.metadata.name, Some(pod_name(job_id)));
    }
}
