use k8s_openapi::api::storage::v1::StorageClass;
use kube::api::{Api, ListParams};

const DEFAULT_STORAGE_CLASS_ANNOTATION: &str = "storageclass.kubernetes.io/is-default-class";

/// Best-effort default StorageClass, to pre-fill `k8s_cache_storage_class`. It's cluster-scoped and needs a
/// ClusterRole beyond the namespaced Role; without one this returns `None` and job submission carries on.
pub async fn detect_default_storage_class(client: &kube::Client) -> Option<String> {
    let api: Api<StorageClass> = Api::all(client.clone());
    let list = match api.list(&ListParams::default()).await {
        Ok(list) => list,
        Err(err) => {
            tracing::debug!(error = %err, "could not list StorageClasses for auto-detection; the admin can still set k8s_cache_storage_class manually");
            return None;
        }
    };
    list.items
        .into_iter()
        .find(|sc| {
            sc.metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get(DEFAULT_STORAGE_CLASS_ANNOTATION))
                .is_some_and(|v| v == "true")
        })
        .and_then(|sc| sc.metadata.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::test_support::{
        TestNamespace, kubectl_apply, scoped_client_for, test_client,
    };

    const FIXTURE: &str = include_str!("rbac-test-only.yaml");

    #[tokio::test]
    async fn detects_the_clusters_annotated_default_storage_class() {
        let client = test_client().await;
        let api: Api<StorageClass> = Api::all(client.clone());
        let all = api
            .list(&ListParams::default())
            .await
            .expect("listing StorageClasses as the unrestricted test client must succeed");
        let expected = all
            .items
            .iter()
            .find(|sc| {
                sc.metadata
                    .annotations
                    .as_ref()
                    .and_then(|a| a.get(DEFAULT_STORAGE_CLASS_ANNOTATION))
                    .is_some_and(|v| v == "true")
            })
            .and_then(|sc| sc.metadata.name.clone());

        let detected = detect_default_storage_class(&client).await;

        assert_eq!(
            detected, expected,
            "must return whichever StorageClass the cluster itself has annotated as default"
        );
    }

    #[tokio::test]
    async fn returns_none_without_erroring_when_the_service_account_cannot_list_storage_classes() {
        // The ferrisgit-ci Role has no storageclasses permission, so this checks detection degrades gracefully.
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        kubectl_apply(&ns.name, FIXTURE).await;
        let scoped = scoped_client_for(&ns.name, "ferrisgit-ci").await;

        let detected = detect_default_storage_class(&scoped).await;

        assert_eq!(detected, None);
    }
}
