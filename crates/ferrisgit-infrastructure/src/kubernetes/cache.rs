use ferrisgit_domain::error::DomainError;
use k8s_openapi::api::core::v1::{
    PersistentVolumeClaim, PersistentVolumeClaimSpec, VolumeResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::Client;
use kube::api::{Api, PostParams};
use std::collections::BTreeMap;
use uuid::Uuid;

/// PVC names must be DNS-1123 subdomains, so free-form cache keys are sanitized; the repository id in
/// the name keeps identical keys from different repositories apart.
pub fn pvc_name(repository_id: Uuid, cache_key: &str) -> String {
    let sanitized: String = cache_key
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("ferrisgit-cache-{repository_id}-{sanitized}")
}

/// Creates one `ReadWriteMany` PVC per declared cache key, if absent. There is no fallback to
/// `ReadWriteOnce`: an unsatisfiable request stays visibly `Pending` instead of silently breaking on
/// a second node. It doesn't wait for `Bound`, which depends on the cluster's `StorageClass`.
pub async fn ensure_cache_pvcs(
    client: &Client,
    namespace: &str,
    repository_id: Uuid,
    cache_keys: &[String],
    storage_class: &str,
) -> Result<(), DomainError> {
    let pvcs: Api<PersistentVolumeClaim> = Api::namespaced(client.clone(), namespace);
    for key in cache_keys {
        let name = pvc_name(repository_id, key);
        if pvcs
            .get_opt(&name)
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?
            .is_some()
        {
            continue;
        }
        let mut requests = BTreeMap::new();
        requests.insert("storage".to_string(), Quantity("5Gi".to_string()));
        let pvc = PersistentVolumeClaim {
            metadata: ObjectMeta {
                name: Some(name),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            spec: Some(PersistentVolumeClaimSpec {
                access_modes: Some(vec!["ReadWriteMany".to_string()]),
                storage_class_name: Some(storage_class.to_string()),
                resources: Some(VolumeResourceRequirements {
                    requests: Some(requests),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        match pvcs.create(&PostParams::default(), &pvc).await {
            Ok(_) => {}
            Err(kube::Error::Api(err)) if err.code == 409 => {} // created concurrently between our get_opt and create — fine
            Err(err) => return Err(DomainError::Infrastructure(err.to_string())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::test_support::{TestNamespace, test_client};

    #[test]
    fn the_same_repository_and_key_always_produce_the_same_name() {
        let repo = Uuid::new_v4();
        assert_eq!(
            pvc_name(repo, "cargo-registry"),
            pvc_name(repo, "cargo-registry")
        );
    }

    #[test]
    fn different_repositories_with_the_same_key_produce_different_names() {
        assert_ne!(
            pvc_name(Uuid::new_v4(), "cargo-registry"),
            pvc_name(Uuid::new_v4(), "cargo-registry")
        );
    }

    #[test]
    fn non_dns_safe_characters_in_the_cache_key_are_sanitized() {
        let name = pvc_name(Uuid::new_v4(), "Cargo Registry/v2");
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[tokio::test]
    async fn ensure_cache_pvcs_creates_one_pvc_per_cache_key_with_the_right_access_mode_and_storage_class()
     {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let repository_id = Uuid::new_v4();
        let keys = vec!["cargo-registry".to_string(), "target-dir".to_string()];

        ensure_cache_pvcs(&client, &ns.name, repository_id, &keys, "standard")
            .await
            .unwrap();

        let pvcs: Api<PersistentVolumeClaim> = Api::namespaced(client, &ns.name);
        for key in &keys {
            let pvc = pvcs
                .get(&pvc_name(repository_id, key))
                .await
                .expect("PVC should exist");
            let spec = pvc.spec.unwrap();
            assert_eq!(
                spec.access_modes.unwrap(),
                vec!["ReadWriteMany".to_string()]
            );
            assert_eq!(spec.storage_class_name.as_deref(), Some("standard"));
        }
    }

    #[tokio::test]
    async fn ensure_cache_pvcs_is_idempotent_when_the_pvc_already_exists() {
        let client = test_client().await;
        let ns = TestNamespace::create(&client).await;
        let repository_id = Uuid::new_v4();
        let keys = vec!["cargo-registry".to_string()];

        ensure_cache_pvcs(&client, &ns.name, repository_id, &keys, "standard")
            .await
            .unwrap();
        ensure_cache_pvcs(&client, &ns.name, repository_id, &keys, "standard")
            .await
            .unwrap();
    }
}
