use crate::kubernetes::cache::pvc_name;
use ferrisgit_domain::job::Job;
use k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource;
use k8s_openapi::api::core::v1::{Container, EnvVar, Pod, PodSpec, Volume, VolumeMount};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const JOB_ID_LABEL: &str = "ferrisgit.io/job-id";
const CACHE_MOUNT_ROOT: &str = "/ferrisgit-cache";

/// The one place a job's Pod name is built, so `cancel`'s lookup can't drift from `build_pod`.
pub fn pod_name(job_id: Uuid) -> String {
    format!("ferrisgit-job-{job_id}")
}

pub fn build_pod(job: &Job, repository_id: Uuid, namespace: &str) -> Pod {
    let mut labels = BTreeMap::new();
    labels.insert(JOB_ID_LABEL.to_string(), job.id.to_string());

    let env: Vec<EnvVar> = job
        .variables
        .iter()
        .map(|(k, v)| EnvVar {
            name: k.clone(),
            value: Some(v.clone()),
            ..Default::default()
        })
        .collect();

    let script = job.script.join(" && ");

    let (volumes, volume_mounts) = if job.cache.is_empty() {
        (None, None)
    } else {
        let volumes: Vec<Volume> = job
            .cache
            .iter()
            .enumerate()
            .map(|(i, key)| Volume {
                name: format!("cache-{i}"),
                persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                    claim_name: pvc_name(repository_id, key),
                    read_only: Some(false),
                }),
                ..Default::default()
            })
            .collect();
        let mounts: Vec<VolumeMount> = job
            .cache
            .iter()
            .enumerate()
            .map(|(i, key)| VolumeMount {
                name: format!("cache-{i}"),
                mount_path: format!("{CACHE_MOUNT_ROOT}/{key}"),
                ..Default::default()
            })
            .collect();
        (Some(volumes), Some(mounts))
    };

    Pod {
        metadata: ObjectMeta {
            name: Some(pod_name(job.id)),
            namespace: Some(namespace.to_string()),
            labels: Some(labels),
            ..Default::default()
        },
        spec: Some(PodSpec {
            restart_policy: Some("Never".to_string()),
            containers: vec![Container {
                name: "job".to_string(),
                image: Some(job.image.clone()),
                command: Some(vec!["/bin/sh".to_string(), "-c".to_string()]),
                args: Some(vec![script]),
                env: Some(env),
                volume_mounts,
                ..Default::default()
            }],
            volumes,
            ..Default::default()
        }),
        ..Default::default()
    }
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
            variables: BTreeMap::from([("RUST_LOG".to_string(), "debug".to_string())]),
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

    #[test]
    fn the_pod_is_labeled_with_the_job_id_for_the_watcher_to_find_later() {
        let job = fake_job(vec![]);
        let pod = build_pod(&job, Uuid::new_v4(), "ferrisgit-jobs");
        assert_eq!(
            pod.metadata.labels.unwrap().get(JOB_ID_LABEL),
            Some(&job.id.to_string())
        );
    }

    #[test]
    fn the_pod_runs_in_the_given_namespace_and_never_restarts() {
        let job = fake_job(vec![]);
        let pod = build_pod(&job, Uuid::new_v4(), "ferrisgit-jobs");
        assert_eq!(pod.metadata.namespace.as_deref(), Some("ferrisgit-jobs"));
        assert_eq!(
            pod.spec.as_ref().unwrap().restart_policy.as_deref(),
            Some("Never")
        );
    }

    #[test]
    fn the_container_uses_the_jobs_image_and_runs_every_script_line() {
        let job = fake_job(vec![]);
        let pod = build_pod(&job, Uuid::new_v4(), "ferrisgit-jobs");
        let container = &pod.spec.unwrap().containers[0];
        assert_eq!(container.image.as_deref(), Some("rust:1.80"));
        let command = container.args.as_ref().unwrap().join(" ");
        assert!(command.contains("cargo build"));
        assert!(command.contains("cargo test"));
    }

    #[test]
    fn job_variables_become_container_env_vars() {
        let job = fake_job(vec![]);
        let pod = build_pod(&job, Uuid::new_v4(), "ferrisgit-jobs");
        let container = &pod.spec.unwrap().containers[0];
        let env = container.env.as_ref().unwrap();
        assert!(
            env.iter()
                .any(|e| e.name == "RUST_LOG" && e.value.as_deref() == Some("debug"))
        );
    }

    #[test]
    fn each_cache_key_mounts_its_own_pvc_at_a_distinct_path() {
        let repository_id = Uuid::new_v4();
        let job = fake_job(vec!["cargo-registry".to_string(), "target-dir".to_string()]);
        let pod = build_pod(&job, repository_id, "ferrisgit-jobs");
        let spec = pod.spec.unwrap();
        let volumes = spec.volumes.unwrap();
        assert_eq!(volumes.len(), 2);
        let claim_names: Vec<_> = volumes
            .iter()
            .map(|v| {
                v.persistent_volume_claim
                    .as_ref()
                    .unwrap()
                    .claim_name
                    .clone()
            })
            .collect();
        assert!(claim_names.contains(&pvc_name(repository_id, "cargo-registry")));
        assert!(claim_names.contains(&pvc_name(repository_id, "target-dir")));
        let mounts = &spec.containers[0].volume_mounts.as_ref().unwrap();
        assert_eq!(mounts.len(), 2);
        assert_ne!(
            mounts[0].mount_path, mounts[1].mount_path,
            "each cache mounts at its own path, never overlapping"
        );
    }

    #[test]
    fn a_job_with_no_cache_keys_gets_no_volumes() {
        let job = fake_job(vec![]);
        let pod = build_pod(&job, Uuid::new_v4(), "ferrisgit-jobs");
        assert!(pod.spec.unwrap().volumes.is_none());
    }
}
