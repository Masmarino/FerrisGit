#[cfg(test)]
mod tests {
    use crate::kubernetes::cache::ensure_cache_pvcs;
    use crate::kubernetes::pod_spec::{build_pod, pod_name};
    use crate::kubernetes::test_support::{
        TestNamespace, kubectl_apply, scoped_client_for, test_client,
    };
    use chrono::Utc;
    use ferrisgit_domain::job::{Job, JobStatus};
    use k8s_openapi::api::core::v1::{PersistentVolumeClaim, Pod, Secret};
    use kube::api::{Api, DeleteParams, ListParams, LogParams, PostParams};
    use std::collections::BTreeMap;
    use uuid::Uuid;

    const FIXTURE: &str = include_str!("rbac-test-only.yaml");

    fn job() -> Job {
        Job {
            id: Uuid::new_v4(),
            pipeline_id: Uuid::new_v4(),
            stage: "build".to_string(),
            name: "compile".to_string(),
            image: "busybox:1.36".to_string(),
            script: vec!["true".to_string()],
            variables: BTreeMap::new(),
            needs: vec![],
            tags: vec![],
            status: JobStatus::Pending,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            cache: vec![],
            started_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn the_documented_role_is_sufficient_for_everything_ferrisgit_does_with_pods_and_pvcs() {
        let cluster_client = test_client().await;
        let ns = TestNamespace::create(&cluster_client).await;
        kubectl_apply(&ns.name, FIXTURE).await;
        let scoped = scoped_client_for(&ns.name, "ferrisgit-ci").await;

        let the_job = job();
        let pod = build_pod(&the_job, Uuid::new_v4(), &ns.name);
        let pods: Api<Pod> = Api::namespaced(scoped.clone(), &ns.name);
        pods.create(&PostParams::default(), &pod)
            .await
            .expect("create pod must be permitted");
        pods.get(&pod_name(the_job.id))
            .await
            .expect("get pod must be permitted");
        pods.list(&ListParams::default())
            .await
            .expect("list pods must be permitted");
        // A 400 ("not ready yet") is fine, but a 403 means the `get` verb on `pods/log` is missing.
        if let Err(err) = pods
            .logs(&pod_name(the_job.id), &LogParams::default())
            .await
        {
            let is_forbidden = matches!(&err, kube::Error::Api(api_err) if api_err.code == 403);
            assert!(
                !is_forbidden,
                "fetching pods/log must not be forbidden by RBAC: {err}"
            );
        }
        pods.delete(&pod_name(the_job.id), &DeleteParams::default())
            .await
            .expect("delete pod must be permitted");

        let pvcs: Api<PersistentVolumeClaim> = Api::namespaced(scoped.clone(), &ns.name);
        pvcs.list(&ListParams::default())
            .await
            .expect("list pvcs must be permitted");
        // `list` doesn't prove `create`: provision a PVC through the production path.
        ensure_cache_pvcs(
            &scoped,
            &ns.name,
            Uuid::new_v4(),
            &["cargo-registry".to_string()],
            "standard",
        )
        .await
        .expect("create pvc must be permitted");

        let secrets: Api<Secret> = Api::namespaced(scoped, &ns.name);
        let forbidden = secrets.list(&ListParams::default()).await;
        assert!(
            forbidden.is_err(),
            "the role must NOT grant access to Secrets — it was never asked for"
        );
    }
}
