//! Test-only doubles shared across `use_cases`' unit tests.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ferrisgit_domain::api_token::{ApiToken, ApiTokenRepositoryPort, NewApiToken};
use ferrisgit_domain::audit::{EventPublisherPort, SecurityEvent};
use ferrisgit_domain::branch::{BranchInfo, BranchReaderPort};
use ferrisgit_domain::diff::{DiffReaderPort, FileDiff};
use ferrisgit_domain::email::{EmailPort, SmtpSecurity, SmtpSettings, SmtpSettingsPort};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::{Group, GroupMember, GroupStorePort, GroupWithPath, NewGroup};
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::health::{
    DatabaseHealth, HealthCheckPort, StorageHealth, StorageHealthCheckPort,
};
use ferrisgit_domain::invitation::{
    Invitation, PendingAccount, PendingAccountPort, UserInvitationPort,
};
use ferrisgit_domain::issue::{
    Issue, IssueComment, IssueKind, IssueStatus, IssueStorePort, NewIssue, NewIssueComment,
};
use ferrisgit_domain::issue_comment::IssueCommentPort;
use ferrisgit_domain::job::{Job, JobStatus, JobStorePort, NewJob, runnable_jobs};
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::label::{Label, LabelStorePort, NewLabel};
use ferrisgit_domain::merge_request::{
    MergeRequest, MergeRequestReview, MergeRequestReviewPort, MergeRequestStatus,
    MergeRequestStorePort, NewMergeRequest, ReviewDecision,
};
use ferrisgit_domain::merge_request_comment::{
    MergeRequestComment, MergeRequestCommentPort, NewMergeRequestComment,
};
use ferrisgit_domain::merge_request_event::{
    MergeRequestEvent, MergeRequestEventPort, NewMergeRequestEvent,
};
use ferrisgit_domain::metrics_snapshot::{MetricsSnapshot, MetricsSnapshotRepositoryPort};
use ferrisgit_domain::mfa::{BackupCodePort, TotpCredential, TotpCredentialPort};
use ferrisgit_domain::milestone::{Milestone, MilestoneState, MilestoneStorePort, NewMilestone};
use ferrisgit_domain::notification::{NewNotification, Notification, NotificationStorePort};
use ferrisgit_domain::password_reset::{PasswordReset, PasswordResetPort};
use ferrisgit_domain::pipeline::{NewPipeline, Pipeline, PipelineStatus, PipelineStorePort};
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::public_pages::{
    PublicPagesSettings, PublicPagesSettingsPort, PublicPagesSettingsUpdate,
};
use ferrisgit_domain::registration::RegistrationSettingsPort;
use ferrisgit_domain::release::{
    NewRelease, NewReleaseAsset, Release, ReleaseAsset, ReleaseStorePort, ReleaseUpdate,
};
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use ferrisgit_domain::repository::{
    NewRepository, Repository, RepositoryStorePort, RepositoryVisibility,
};
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaborator, RepositoryCollaboratorStorePort,
};
use ferrisgit_domain::runner::{NewRunner, Runner, RunnerRepositoryPort};
use ferrisgit_domain::settings::{
    CiVariable, ExecutionEngine, NewCiVariable, RepositorySettings, RepositorySettingsStorePort,
    RepositorySettingsUpdate, SystemSettings, SystemSettingsStorePort, SystemSettingsUpdate,
};
use ferrisgit_domain::storage_size::DirectorySizePort;
use ferrisgit_domain::tag::TagCreatorPort;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, User, UserRepositoryPort};
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use ferrisgit_domain::webhook::{
    NewWebhook, NewWebhookDelivery, Webhook, WebhookDelivery, WebhookStorePort, WebhookUpdate,
};
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use ferrisgit_domain::wiki::{NewWiki, Wiki, WikiStorePort};
use ferrisgit_domain::wiki_page::{WikiRevision, WikiWriterPort};

/// First matching row, cloned out of the lock.
fn find_in<T: Clone>(rows: &Mutex<Vec<T>>, matches: impl Fn(&T) -> bool) -> Option<T> {
    rows.lock()
        .unwrap()
        .iter()
        .find(|row| matches(row))
        .cloned()
}

/// All matching rows, in insertion order.
fn filter_in<T: Clone>(rows: &Mutex<Vec<T>>, keep: impl Fn(&T) -> bool) -> Vec<T> {
    rows.lock()
        .unwrap()
        .iter()
        .filter(|row| keep(row))
        .cloned()
        .collect()
}

/// Updates the first matching row and does nothing if none matches, like an `UPDATE` hitting zero rows.
fn update_in<T>(rows: &Mutex<Vec<T>>, matches: impl Fn(&T) -> bool, change: impl FnOnce(&mut T)) {
    if let Some(row) = rows.lock().unwrap().iter_mut().find(|row| matches(row)) {
        change(row);
    }
}

/// A SQL `LIMIT` as a count: negative selects nothing.
fn row_limit(limit: i64) -> usize {
    limit.max(0) as usize
}

pub struct FakeUsers {
    users: Mutex<Vec<User>>,
    token_epochs: Mutex<HashMap<Uuid, i32>>,
    fail_next_password_update: Mutex<bool>,
    /// An admin with a pending invitation or reset doesn't count, they can't sign in.
    invitations: Option<Arc<FakeInvitations>>,
    password_resets: Option<Arc<FakePasswordResets>>,
    repositories: Option<Arc<FakeRepositories>>,
}

impl FakeUsers {
    pub fn new(users: Vec<User>) -> Self {
        Self {
            users: Mutex::new(users),
            token_epochs: Mutex::new(HashMap::new()),
            fail_next_password_update: Mutex::new(false),
            invitations: None,
            password_resets: None,
            repositories: None,
        }
    }

    pub fn with_invitations(mut self, invitations: Arc<FakeInvitations>) -> Self {
        self.invitations = Some(invitations);
        self
    }

    pub fn with_password_resets(mut self, password_resets: Arc<FakePasswordResets>) -> Self {
        self.password_resets = Some(password_resets);
        self
    }

    pub fn with_repositories(mut self, repositories: Arc<FakeRepositories>) -> Self {
        self.repositories = Some(repositories);
        self
    }

    /// Admins who can sign in: flag set, no invitation or reset pending.
    fn active_admin_ids(&self, users: &[User]) -> Vec<Uuid> {
        let mut pending: Vec<Uuid> = self
            .invitations
            .as_ref()
            .map(|i| i.snapshot().into_iter().map(|(id, _, _)| id).collect())
            .unwrap_or_default();
        pending.extend(
            self.password_resets
                .as_ref()
                .map(|r| {
                    r.snapshot()
                        .into_iter()
                        .map(|(id, _, _)| id)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        let mut ids: Vec<Uuid> = users
            .iter()
            .filter(|u| u.is_admin && !pending.contains(&u.id))
            .map(|u| u.id)
            .collect();
        ids.sort();
        ids
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn fail_next_password_update(&self) {
        *self.fail_next_password_update.lock().unwrap() = true;
    }

    pub fn get(&self, id: Uuid) -> Option<User> {
        find_in(&self.users, |u| u.id == id)
    }

    pub fn snapshot(&self) -> Vec<User> {
        self.users.lock().unwrap().clone()
    }

    pub fn token_epoch_of(&self, user_id: Uuid) -> i32 {
        *self
            .token_epochs
            .lock()
            .unwrap()
            .get(&user_id)
            .unwrap_or(&0)
    }
}

#[async_trait]
impl UserRepositoryPort for FakeUsers {
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        Ok(find_in(&self.users, |u| u.username == username))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DomainError> {
        Ok(find_in(&self.users, |u| u.id == id))
    }

    async fn create(&self, new_user: NewUser) -> Result<User, DomainError> {
        let user = User {
            id: Uuid::new_v4(),
            username: new_user.username,
            email: new_user.email,
            password_hash: new_user.password_hash,
            is_admin: new_user.is_admin,
            created_at: Utc::now(),
        };
        self.users.lock().unwrap().push(user.clone());
        Ok(user)
    }

    async fn count(&self) -> Result<i64, DomainError> {
        Ok(self.users.lock().unwrap().len() as i64)
    }

    async fn update_email(&self, user_id: Uuid, email: String) -> Result<User, DomainError> {
        let mut users = self.users.lock().unwrap();
        let user = users
            .iter_mut()
            .find(|u| u.id == user_id)
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        user.email = email;
        Ok(user.clone())
    }

    async fn update_password_hash(
        &self,
        user_id: Uuid,
        password_hash: String,
    ) -> Result<(), DomainError> {
        if std::mem::take(&mut *self.fail_next_password_update.lock().unwrap()) {
            return Err(DomainError::Infrastructure(
                "password update failed".to_string(),
            ));
        }
        let mut users = self.users.lock().unwrap();
        let user = users
            .iter_mut()
            .find(|u| u.id == user_id)
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        user.password_hash = password_hash;
        Ok(())
    }

    async fn set_username_and_password_hash(
        &self,
        user_id: Uuid,
        username: String,
        password_hash: String,
    ) -> Result<(), DomainError> {
        if std::mem::take(&mut *self.fail_next_password_update.lock().unwrap()) {
            return Err(DomainError::Infrastructure(
                "password update failed".to_string(),
            ));
        }
        let mut users = self.users.lock().unwrap();
        if users
            .iter()
            .any(|u| u.id != user_id && u.username.eq_ignore_ascii_case(&username))
        {
            return Err(DomainError::Conflict("username already taken".to_string()));
        }
        let user = users
            .iter_mut()
            .find(|u| u.id == user_id)
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        user.username = username;
        user.password_hash = password_hash;
        Ok(())
    }

    async fn count_admins(&self) -> Result<i64, DomainError> {
        Ok(self.active_admin_ids(&self.users.lock().unwrap()).len() as i64)
    }

    /// The last-admin floor is enforced under the one mutex, as the real adapter does with row locks.
    async fn set_admin(&self, user_id: Uuid, is_admin: bool) -> Result<(), DomainError> {
        let mut users = self.users.lock().unwrap();
        if !is_admin && self.active_admin_ids(&users) == [user_id] {
            return Err(DomainError::Conflict(
                "cannot remove the last administrator".to_string(),
            ));
        }
        let user = users
            .iter_mut()
            .find(|u| u.id == user_id)
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        user.is_admin = is_admin;
        Ok(())
    }

    /// Checks `heir_id == user_id`, `NotFound` and the last-admin floor before changing anything. The group Maintainer
    /// rule is only tested against Postgres, this fake knows no groups.
    async fn delete(&self, user_id: Uuid, heir_id: Uuid) -> Result<Vec<Repository>, DomainError> {
        if heir_id == user_id {
            return Err(DomainError::Validation(
                "a deleted user's group repositories cannot go to that same user".to_string(),
            ));
        }
        let mut users = self.users.lock().unwrap();
        if !users.iter().any(|u| u.id == user_id) {
            return Err(DomainError::NotFound("user".to_string()));
        }
        if self.active_admin_ids(&users) == [user_id] {
            return Err(DomainError::Conflict(
                "cannot remove the last administrator".to_string(),
            ));
        }
        let deleted = self
            .repositories
            .as_ref()
            .map(|r| r.remove_owned_by(user_id, heir_id))
            .unwrap_or_default();
        users.retain(|u| u.id != user_id);
        Ok(deleted)
    }

    /// Substring match, not full-text: no negation or ranking.
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<User>, DomainError> {
        let query = query.to_lowercase();
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.username.to_lowercase().contains(&query))
            .take(row_limit(limit))
            .cloned()
            .collect())
    }

    /// `to_lowercase` folds all of Unicode, unlike Postgres `lower()`, so DB tests of the real adapter should stick to
    /// ASCII case pairs.
    async fn find_by_username_ignore_case(
        &self,
        username: &str,
    ) -> Result<Option<User>, DomainError> {
        Ok(find_in(&self.users, |u| {
            u.username.to_lowercase() == username.to_lowercase()
        }))
    }

    async fn find_by_email_ignore_case(&self, email: &str) -> Result<Option<User>, DomainError> {
        Ok(find_in(&self.users, |u| {
            u.email.to_lowercase() == email.to_lowercase()
        }))
    }

    async fn list(&self, limit: i64) -> Result<Vec<User>, DomainError> {
        let mut users = self.users.lock().unwrap().clone();
        users.sort_by_key(|u| (u.created_at, u.id));
        users.truncate(row_limit(limit));
        Ok(users)
    }

    async fn get_token_epoch(&self, user_id: Uuid) -> Result<i32, DomainError> {
        Ok(self.token_epoch_of(user_id))
    }

    async fn bump_token_epoch(&self, user_id: Uuid) -> Result<(), DomainError> {
        *self
            .token_epochs
            .lock()
            .unwrap()
            .entry(user_id)
            .or_insert(0) += 1;
        Ok(())
    }
}

pub struct FakeRepositories {
    repos: Mutex<Vec<Repository>>,
    deleted: Mutex<Vec<Uuid>>,
}

impl FakeRepositories {
    pub fn new(repos: Vec<Repository>) -> Self {
        Self {
            repos: Mutex::new(repos),
            deleted: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Repository> {
        find_in(&self.repos, |r| r.id == id)
    }

    pub fn snapshot(&self) -> Vec<Repository> {
        self.repos.lock().unwrap().clone()
    }

    pub fn deleted_ids(&self) -> Vec<Uuid> {
        self.deleted.lock().unwrap().clone()
    }

    /// Returns and removes the owner's personal repos, and hands their group repos to `heir_id`.
    fn remove_owned_by(&self, owner_id: Uuid, heir_id: Uuid) -> Vec<Repository> {
        let mut repos = self.repos.lock().unwrap();
        let (removed, mut kept): (Vec<Repository>, Vec<Repository>) = repos
            .drain(..)
            .partition(|r| r.owner_id == owner_id && r.group_id.is_none());
        for repo in kept.iter_mut().filter(|r| r.owner_id == owner_id) {
            repo.owner_id = heir_id;
        }
        *repos = kept;
        self.deleted
            .lock()
            .unwrap()
            .extend(removed.iter().map(|r| r.id));
        removed
    }
}

#[async_trait]
impl RepositoryStorePort for FakeRepositories {
    /// Same uniqueness scope as `PostgresRepositoryStore::create`: name per owner among personal repos, per group among a
    /// group's.
    async fn create(
        &self,
        new_repo: NewRepository,
        disk_path: String,
    ) -> Result<Repository, DomainError> {
        let mut repos = self.repos.lock().unwrap();
        let conflict = match new_repo.group_id {
            Some(group_id) => repos
                .iter()
                .any(|r| r.group_id == Some(group_id) && r.name == new_repo.name),
            None => repos.iter().any(|r| {
                r.owner_id == new_repo.owner_id && r.group_id.is_none() && r.name == new_repo.name
            }),
        };
        if conflict {
            return Err(match new_repo.group_id {
                Some(_) => DomainError::Conflict(format!(
                    "a repository named '{}' already exists in this group",
                    new_repo.name
                )),
                None => DomainError::Conflict(format!(
                    "a repository named '{}' already exists",
                    new_repo.name
                )),
            });
        }
        let repo = Repository {
            id: Uuid::new_v4(),
            owner_id: new_repo.owner_id,
            name: new_repo.name,
            group_id: new_repo.group_id,
            description: new_repo.description,
            disk_path,
            visibility: new_repo.visibility,
            created_at: Utc::now(),
        };
        repos.push(repo.clone());
        Ok(repo)
    }

    async fn list_for_owner(&self, owner_id: Uuid) -> Result<Vec<Repository>, DomainError> {
        Ok(filter_in(&self.repos, |r| {
            r.owner_id == owner_id && r.group_id.is_none()
        }))
    }

    async fn find_by_owner_and_name(
        &self,
        owner_id: Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        Ok(find_in(&self.repos, |r| {
            r.owner_id == owner_id && r.group_id.is_none() && r.name == name
        }))
    }

    async fn find_by_group_and_name(
        &self,
        group_id: Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        Ok(find_in(&self.repos, |r| {
            r.group_id == Some(group_id) && r.name == name
        }))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Repository>, DomainError> {
        Ok(find_in(&self.repos, |r| r.id == id))
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.repos.lock().unwrap().retain(|r| r.id != id);
        self.deleted.lock().unwrap().push(id);
        Ok(())
    }

    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Repository>, DomainError> {
        let mut repos: Vec<Repository> = filter_in(&self.repos, |r| r.group_id == Some(group_id));
        repos.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(repos)
    }

    async fn list_all(&self) -> Result<Vec<Repository>, DomainError> {
        Ok(self.repos.lock().unwrap().clone())
    }

    async fn list_public(&self) -> Result<Vec<Uuid>, DomainError> {
        Ok(self
            .repos
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.visibility == RepositoryVisibility::Public)
            .map(|r| r.id)
            .collect())
    }

    /// Substring match, not full-text: no negation or ranking.
    async fn search(
        &self,
        ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<Repository>, DomainError> {
        let query = query.to_lowercase();
        Ok(self
            .repos
            .lock()
            .unwrap()
            .iter()
            .filter(|r| {
                ids.contains(&r.id)
                    && (r.name.to_lowercase().contains(&query)
                        || r.description.to_lowercase().contains(&query))
            })
            .take(row_limit(limit))
            .cloned()
            .collect())
    }
}

pub struct FakeGroups {
    groups: Mutex<Vec<Group>>,
    members: Mutex<Vec<(Uuid, Uuid, CollaboratorRole)>>,
    deleted: Mutex<Vec<Uuid>>,
    set_member_role_calls: Mutex<Vec<(Uuid, Uuid, CollaboratorRole)>>,
    /// Forces the result of `delete_if_empty`, to simulate the group filling up between the use case's checks and the
    /// atomic delete.
    delete_if_empty_override: Mutex<Option<bool>>,
}

impl FakeGroups {
    pub fn new(groups: Vec<Group>) -> Self {
        Self {
            groups: Mutex::new(groups),
            members: Mutex::new(vec![]),
            deleted: Mutex::new(vec![]),
            set_member_role_calls: Mutex::new(vec![]),
            delete_if_empty_override: Mutex::new(None),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Group> {
        find_in(&self.groups, |g| g.id == id)
    }

    pub fn has_member(&self, group_id: Uuid, user_id: Uuid, role: CollaboratorRole) -> bool {
        self.members
            .lock()
            .unwrap()
            .contains(&(group_id, user_id, role))
    }

    pub fn is_member(&self, group_id: Uuid, user_id: Uuid) -> bool {
        self.members
            .lock()
            .unwrap()
            .iter()
            .any(|(g, u, _)| *g == group_id && *u == user_id)
    }

    pub fn deleted_ids(&self) -> Vec<Uuid> {
        self.deleted.lock().unwrap().clone()
    }

    pub fn set_member_role_call_count(&self) -> usize {
        self.set_member_role_calls.lock().unwrap().len()
    }

    pub fn force_delete_if_empty(&self, result: bool) {
        *self.delete_if_empty_override.lock().unwrap() = Some(result);
    }

    /// Root first, `group_id` included.
    fn chain_within(groups: &[Group], group_id: Uuid) -> Vec<Group> {
        let mut chain = vec![];
        let mut current = groups.iter().find(|g| g.id == group_id).cloned();
        while let Some(g) = current {
            let parent = g.parent_group_id;
            chain.push(g);
            current = parent.and_then(|pid| groups.iter().find(|g| g.id == pid).cloned());
        }
        chain.reverse();
        chain
    }

    fn descendants_of(groups: &[Group], root_id: Uuid) -> Vec<Uuid> {
        let mut result = vec![];
        let mut stack = vec![root_id];
        while let Some(parent_id) = stack.pop() {
            for child in groups
                .iter()
                .filter(|g| g.parent_group_id == Some(parent_id))
            {
                result.push(child.id);
                stack.push(child.id);
            }
        }
        result
    }
}

#[async_trait]
impl GroupStorePort for FakeGroups {
    async fn create(&self, new_group: NewGroup) -> Result<Group, DomainError> {
        let group = Group {
            id: Uuid::new_v4(),
            parent_group_id: new_group.parent_group_id,
            name: new_group.name,
            description: new_group.description,
            created_by: Some(new_group.created_by),
            created_at: Utc::now(),
        };
        self.groups.lock().unwrap().push(group.clone());
        Ok(group)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Group>, DomainError> {
        Ok(find_in(&self.groups, |g| g.id == id))
    }

    async fn find_child_by_name(
        &self,
        parent_id: Option<Uuid>,
        name: &str,
    ) -> Result<Option<Group>, DomainError> {
        Ok(find_in(&self.groups, |g| {
            g.parent_group_id == parent_id && g.name == name
        }))
    }

    async fn list_children(&self, parent_id: Option<Uuid>) -> Result<Vec<Group>, DomainError> {
        Ok(filter_in(&self.groups, |g| g.parent_group_id == parent_id))
    }

    async fn ancestor_chain(&self, group_id: Uuid) -> Result<Vec<Group>, DomainError> {
        Ok(Self::chain_within(&self.groups.lock().unwrap(), group_id))
    }

    /// Groups where the user is a direct Maintainer plus all descendants, deduplicated. `path` joins the root-first ancestor
    /// names with `/`, like the real store.
    async fn list_writable_groups(&self, user_id: Uuid) -> Result<Vec<GroupWithPath>, DomainError> {
        let groups = self.groups.lock().unwrap().clone();
        let maintainer_root_ids: Vec<Uuid> = self
            .members
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, u, r)| *u == user_id && *r == CollaboratorRole::Maintainer)
            .map(|(g, _, _)| *g)
            .collect();

        let mut result: Vec<GroupWithPath> = vec![];
        let mut seen = HashSet::new();
        for root_id in maintainer_root_ids {
            let in_tree = std::iter::once(root_id).chain(Self::descendants_of(&groups, root_id));
            for group_id in in_tree {
                let Some(group) = groups.iter().find(|g| g.id == group_id) else {
                    continue;
                };
                if seen.insert(group_id) {
                    let path = Self::chain_within(&groups, group_id)
                        .iter()
                        .map(|g| g.name.as_str())
                        .collect::<Vec<_>>()
                        .join("/");
                    result.push(GroupWithPath {
                        group: group.clone(),
                        path,
                    });
                }
            }
        }
        Ok(result)
    }

    /// Groups with any direct role plus all descendants, since permissions inherit downward whatever the role.
    async fn list_member_group_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        let groups = self.groups.lock().unwrap().clone();
        let direct_ids: Vec<Uuid> = self
            .members
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, u, _)| *u == user_id)
            .map(|(g, _, _)| *g)
            .collect();

        let mut result = vec![];
        let mut seen = HashSet::new();
        for root_id in direct_ids {
            if seen.insert(root_id) {
                result.push(root_id);
            }
            for descendant_id in Self::descendants_of(&groups, root_id) {
                if seen.insert(descendant_id) {
                    result.push(descendant_id);
                }
            }
        }
        Ok(result)
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.deleted.lock().unwrap().push(id);
        Ok(())
    }

    /// Only checks child groups. The repository half of "empty" is covered by the repository fakes rejecting first, and
    /// `force_delete_if_empty` simulates the race.
    async fn delete_if_empty(&self, id: Uuid) -> Result<bool, DomainError> {
        if let Some(forced) = *self.delete_if_empty_override.lock().unwrap() {
            if forced {
                self.deleted.lock().unwrap().push(id);
            }
            return Ok(forced);
        }
        let empty = !self
            .groups
            .lock()
            .unwrap()
            .iter()
            .any(|g| g.parent_group_id == Some(id));
        if empty {
            self.deleted.lock().unwrap().push(id);
        }
        Ok(empty)
    }
}

#[async_trait]
impl GroupMembershipPort for FakeGroups {
    async fn add_member(
        &self,
        group_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        self.members.lock().unwrap().push((group_id, user_id, role));
        Ok(())
    }

    async fn set_member_role(
        &self,
        group_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        self.set_member_role_calls
            .lock()
            .unwrap()
            .push((group_id, user_id, role));
        let mut members = self.members.lock().unwrap();
        let Some(m) = members
            .iter_mut()
            .find(|(g, u, _)| *g == group_id && *u == user_id)
        else {
            return Err(DomainError::NotFound("group member".to_string()));
        };
        m.2 = role;
        Ok(())
    }

    async fn remove_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        self.members
            .lock()
            .unwrap()
            .retain(|(g, u, _)| !(*g == group_id && *u == user_id));
        Ok(())
    }

    async fn list_members(&self, group_id: Uuid) -> Result<Vec<GroupMember>, DomainError> {
        Ok(self
            .members
            .lock()
            .unwrap()
            .iter()
            .filter(|(g, _, _)| *g == group_id)
            .map(|(g, u, r)| GroupMember {
                group_id: *g,
                user_id: *u,
                username: "someone".to_string(),
                role: *r,
                created_at: Utc::now(),
            })
            .collect())
    }

    async fn get_member_role(
        &self,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError> {
        Ok(self
            .members
            .lock()
            .unwrap()
            .iter()
            .find(|(g, u, _)| *g == group_id && *u == user_id)
            .map(|(_, _, r)| *r))
    }
}

pub struct FakeIssues {
    issues: Mutex<Vec<Issue>>,
    comments: Mutex<Vec<IssueComment>>,
    list_assigned_to_calls: Mutex<Vec<(Uuid, Vec<Uuid>)>>,
}

impl FakeIssues {
    pub fn new(issues: Vec<Issue>) -> Self {
        Self {
            issues: Mutex::new(issues),
            comments: Mutex::new(vec![]),
            list_assigned_to_calls: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Issue> {
        find_in(&self.issues, |i| i.id == id)
    }

    pub fn snapshot(&self) -> Vec<Issue> {
        self.issues.lock().unwrap().clone()
    }

    /// Records calls, because correct filtering here would return the same rows for a wrongly scoped call.
    pub fn list_assigned_to_calls(&self) -> Vec<(Uuid, Vec<Uuid>)> {
        self.list_assigned_to_calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl IssueStorePort for FakeIssues {
    async fn create(&self, new_issue: NewIssue) -> Result<Issue, DomainError> {
        let mut issues = self.issues.lock().unwrap();
        let number = issues
            .iter()
            .filter(|i| i.repository_id == new_issue.repository_id)
            .count() as i32
            + 1;
        let issue = Issue {
            id: Uuid::new_v4(),
            repository_id: new_issue.repository_id,
            number,
            author_id: new_issue.author_id,
            assignee_id: None,
            milestone_id: None,
            title: new_issue.title,
            description: new_issue.description,
            status: IssueStatus::Todo,
            kind: new_issue.kind,
            parent_issue_id: new_issue.parent_issue_id,
            created_at: Utc::now(),
            closed_at: None,
        };
        issues.push(issue.clone());
        Ok(issue)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Issue>, DomainError> {
        Ok(find_in(&self.issues, |i| i.id == id))
    }

    async fn find_by_number(
        &self,
        repository_id: Uuid,
        number: i32,
    ) -> Result<Option<Issue>, DomainError> {
        Ok(find_in(&self.issues, |i| {
            i.repository_id == repository_id && i.number == number
        }))
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Issue>, DomainError> {
        let mut issues: Vec<Issue> = filter_in(&self.issues, |i| i.repository_id == repository_id);
        issues.sort_by_key(|i| i.number);
        Ok(issues)
    }

    /// Filters by `milestone_id` only. There's no issue-label association here, so a non-empty `label_ids` narrows to
    /// nothing.
    async fn list_for_repository_filtered(
        &self,
        repository_id: Uuid,
        label_ids: Option<Vec<Uuid>>,
        milestone_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, DomainError> {
        if label_ids.is_some_and(|ids| !ids.is_empty()) {
            return Ok(vec![]);
        }
        let mut issues: Vec<Issue> = filter_in(&self.issues, |i| {
            i.repository_id == repository_id
                && milestone_id.is_none_or(|m| i.milestone_id == Some(m))
        });
        issues.sort_by_key(|i| i.number);
        Ok(issues)
    }

    /// Silent no-op for an unknown `id`, like an `UPDATE` matching zero rows.
    async fn update_fields(
        &self,
        id: Uuid,
        title: String,
        description: String,
        kind: IssueKind,
    ) -> Result<(), DomainError> {
        update_in(
            &self.issues,
            |i| i.id == id,
            |issue| {
                issue.title = title;
                issue.description = description;
                issue.kind = kind;
            },
        );
        Ok(())
    }

    async fn update_status(&self, id: Uuid, status: IssueStatus) -> Result<(), DomainError> {
        update_in(
            &self.issues,
            |i| i.id == id,
            |issue| {
                issue.closed_at = if status == IssueStatus::Done {
                    issue.closed_at.or_else(|| Some(Utc::now()))
                } else {
                    None
                };
                issue.status = status;
            },
        );
        Ok(())
    }

    async fn assign(&self, id: Uuid, assignee_id: Option<Uuid>) -> Result<(), DomainError> {
        update_in(
            &self.issues,
            |i| i.id == id,
            |issue| issue.assignee_id = assignee_id,
        );
        Ok(())
    }

    async fn set_milestone(&self, id: Uuid, milestone_id: Option<Uuid>) -> Result<(), DomainError> {
        update_in(
            &self.issues,
            |i| i.id == id,
            |issue| issue.milestone_id = milestone_id,
        );
        Ok(())
    }

    async fn close(&self, id: Uuid) -> Result<(), DomainError> {
        update_in(
            &self.issues,
            |i| i.id == id,
            |issue| {
                issue.status = IssueStatus::Done;
                issue.closed_at = Some(Utc::now());
            },
        );
        Ok(())
    }

    async fn reopen(&self, id: Uuid) -> Result<(), DomainError> {
        update_in(
            &self.issues,
            |i| i.id == id,
            |issue| {
                issue.status = IssueStatus::Todo;
                issue.closed_at = None;
            },
        );
        Ok(())
    }

    /// Substring match, not full-text.
    async fn search(
        &self,
        repository_ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError> {
        let query = query.to_lowercase();
        Ok(self
            .issues
            .lock()
            .unwrap()
            .iter()
            .filter(|i| {
                repository_ids.contains(&i.repository_id)
                    && (i.title.to_lowercase().contains(&query)
                        || i.description.to_lowercase().contains(&query))
            })
            .take(row_limit(limit))
            .cloned()
            .collect())
    }

    async fn list_assigned_to(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError> {
        self.list_assigned_to_calls
            .lock()
            .unwrap()
            .push((user_id, repository_ids.to_vec()));
        let mut issues: Vec<Issue> = filter_in(&self.issues, |i| {
            i.assignee_id == Some(user_id)
                && i.status != IssueStatus::Done
                && repository_ids.contains(&i.repository_id)
        });
        issues.sort_by_key(|i| Reverse(i.created_at));
        issues.truncate(row_limit(limit));
        Ok(issues)
    }

    async fn list_authored_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<Issue>, DomainError> {
        let mut issues: Vec<Issue> = filter_in(&self.issues, |i| {
            i.author_id == user_id
                && i.status != IssueStatus::Done
                && repository_ids.contains(&i.repository_id)
        });
        issues.sort_by_key(|i| Reverse(i.created_at));
        issues.truncate(row_limit(limit));
        Ok(issues)
    }
}

#[async_trait]
impl IssueCommentPort for FakeIssues {
    async fn add_comment(&self, new_comment: NewIssueComment) -> Result<IssueComment, DomainError> {
        let comment = IssueComment {
            id: Uuid::new_v4(),
            issue_id: new_comment.issue_id,
            author_id: new_comment.author_id,
            body: new_comment.body,
            created_at: Utc::now(),
        };
        self.comments.lock().unwrap().push(comment.clone());
        Ok(comment)
    }

    async fn list_comments(&self, issue_id: Uuid) -> Result<Vec<IssueComment>, DomainError> {
        Ok(filter_in(&self.comments, |c| c.issue_id == issue_id))
    }
}

pub struct FakeNotifications {
    notifications: Mutex<Vec<Notification>>,
}

impl FakeNotifications {
    pub fn new(notifications: Vec<Notification>) -> Self {
        Self {
            notifications: Mutex::new(notifications),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn snapshot(&self) -> Vec<Notification> {
        self.notifications.lock().unwrap().clone()
    }
}

#[async_trait]
impl NotificationStorePort for FakeNotifications {
    async fn create(&self, notification: NewNotification) -> Result<(), DomainError> {
        let notification = Notification {
            id: Uuid::new_v4(),
            recipient_id: notification.recipient_id,
            kind: notification.kind,
            repository_owner: notification.repository_owner,
            repository_name: notification.repository_name,
            actor_username: notification.actor_username,
            merge_request_id: notification.merge_request_id,
            merge_request_title: notification.merge_request_title,
            pipeline_id: notification.pipeline_id,
            commit_sha: notification.commit_sha,
            issue_id: notification.issue_id,
            issue_number: notification.issue_number,
            issue_title: notification.issue_title,
            role: notification.role,
            read_at: None,
            created_at: Utc::now(),
        };
        self.notifications.lock().unwrap().push(notification);
        Ok(())
    }

    async fn list_for_recipient(
        &self,
        recipient_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        let mut notifications: Vec<Notification> =
            filter_in(&self.notifications, |n| n.recipient_id == recipient_id);
        notifications.sort_by_key(|n| Reverse(n.created_at));
        notifications.truncate(row_limit(limit));
        Ok(notifications)
    }

    async fn unread_count(&self, recipient_id: Uuid) -> Result<i64, DomainError> {
        Ok(self
            .notifications
            .lock()
            .unwrap()
            .iter()
            .filter(|n| n.recipient_id == recipient_id && n.read_at.is_none())
            .count() as i64)
    }

    /// Succeeds even when nothing matches: wrong recipient, unknown id or already read.
    async fn mark_read(
        &self,
        notification_id: Uuid,
        recipient_id: Uuid,
    ) -> Result<(), DomainError> {
        update_in(
            &self.notifications,
            |n| n.id == notification_id && n.recipient_id == recipient_id && n.read_at.is_none(),
            |n| n.read_at = Some(Utc::now()),
        );
        Ok(())
    }

    async fn mark_all_read(&self, recipient_id: Uuid) -> Result<(), DomainError> {
        for n in self
            .notifications
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|n| n.recipient_id == recipient_id && n.read_at.is_none())
        {
            n.read_at = Some(Utc::now());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeWebhooks {
    dispatched: Mutex<Vec<(Uuid, WebhookEvent)>>,
}

impl FakeWebhooks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dispatched(&self) -> Vec<(Uuid, WebhookEvent)> {
        self.dispatched.lock().unwrap().clone()
    }
}

#[async_trait]
impl WebhookDispatcherPort for FakeWebhooks {
    async fn dispatch(&self, repository_id: Uuid, event: WebhookEvent) -> Result<(), DomainError> {
        self.dispatched.lock().unwrap().push((repository_id, event));
        Ok(())
    }
}

pub struct FakeMergeRequests {
    merge_requests: Mutex<Vec<MergeRequest>>,
    comments: Mutex<Vec<MergeRequestComment>>,
    reviews: Mutex<Vec<MergeRequestReview>>,
    list_awaiting_review_by_calls: Mutex<Vec<(Uuid, Vec<Uuid>)>>,
}

impl FakeMergeRequests {
    pub fn new(merge_requests: Vec<MergeRequest>) -> Self {
        Self {
            merge_requests: Mutex::new(merge_requests),
            comments: Mutex::new(vec![]),
            reviews: Mutex::new(vec![]),
            list_awaiting_review_by_calls: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    /// Seeds review rows directly, skipping `upsert_review`'s overwrite rules and timestamps.
    pub fn with_reviews(self, reviews: Vec<MergeRequestReview>) -> Self {
        *self.reviews.lock().unwrap() = reviews;
        self
    }

    pub fn with_comments(self, comments: Vec<MergeRequestComment>) -> Self {
        *self.comments.lock().unwrap() = comments;
        self
    }

    pub fn seed_comment(&self, comment: MergeRequestComment) {
        self.comments.lock().unwrap().push(comment);
    }

    pub fn get(&self, id: Uuid) -> Option<MergeRequest> {
        find_in(&self.merge_requests, |m| m.id == id)
    }

    pub fn snapshot(&self) -> Vec<MergeRequest> {
        self.merge_requests.lock().unwrap().clone()
    }

    pub fn reviews_snapshot(&self) -> Vec<MergeRequestReview> {
        self.reviews.lock().unwrap().clone()
    }

    /// Records calls to check the scoping a use case passed down.
    pub fn list_awaiting_review_by_calls(&self) -> Vec<(Uuid, Vec<Uuid>)> {
        self.list_awaiting_review_by_calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl MergeRequestStorePort for FakeMergeRequests {
    async fn create(&self, new_mr: NewMergeRequest) -> Result<MergeRequest, DomainError> {
        let mr = MergeRequest {
            id: Uuid::new_v4(),
            repository_id: new_mr.repository_id,
            author_id: Some(new_mr.author_id),
            source_branch: new_mr.source_branch,
            target_branch: new_mr.target_branch,
            title: new_mr.title,
            description: new_mr.description,
            status: MergeRequestStatus::Open,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
        };
        self.merge_requests.lock().unwrap().push(mr.clone());
        Ok(mr)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<MergeRequest>, DomainError> {
        Ok(find_in(&self.merge_requests, |m| m.id == id))
    }

    /// Filters by `milestone_id` only. There's no merge-request-label association here, so a non-empty `label_ids` narrows
    /// to nothing.
    async fn list_for_repository_filtered(
        &self,
        repository_id: Uuid,
        label_ids: Option<Vec<Uuid>>,
        milestone_id: Option<Uuid>,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        if label_ids.is_some_and(|ids| !ids.is_empty()) {
            return Ok(vec![]);
        }
        let mut mrs: Vec<MergeRequest> = filter_in(&self.merge_requests, |m| {
            m.repository_id == repository_id
                && milestone_id.is_none_or(|mid| m.milestone_id == Some(mid))
        });
        mrs.sort_by_key(|m| Reverse(m.created_at));
        Ok(mrs)
    }

    async fn update_fields(
        &self,
        id: Uuid,
        title: String,
        description: String,
    ) -> Result<(), DomainError> {
        update_in(
            &self.merge_requests,
            |m| m.id == id,
            |mr| {
                mr.title = title;
                mr.description = description;
            },
        );
        Ok(())
    }

    async fn set_milestone(&self, id: Uuid, milestone_id: Option<Uuid>) -> Result<(), DomainError> {
        update_in(
            &self.merge_requests,
            |m| m.id == id,
            |mr| mr.milestone_id = milestone_id,
        );
        Ok(())
    }

    /// `Conflict` unless the merge request exists and is open, like the real `AND status = 'open'`.
    async fn mark_merged(&self, id: Uuid, merge_commit_sha: &str) -> Result<(), DomainError> {
        let mut merge_requests = self.merge_requests.lock().unwrap();
        match merge_requests
            .iter_mut()
            .find(|m| m.id == id && m.status == MergeRequestStatus::Open)
        {
            Some(mr) => {
                mr.status = MergeRequestStatus::Merged;
                mr.merge_commit_sha = Some(merge_commit_sha.to_string());
                mr.closed_at = Some(Utc::now());
                Ok(())
            }
            None => Err(DomainError::Conflict(
                "merge request is no longer open".to_string(),
            )),
        }
    }

    async fn mark_closed(&self, id: Uuid) -> Result<(), DomainError> {
        let mut merge_requests = self.merge_requests.lock().unwrap();
        match merge_requests
            .iter_mut()
            .find(|m| m.id == id && m.status == MergeRequestStatus::Open)
        {
            Some(mr) => {
                mr.status = MergeRequestStatus::Closed;
                mr.closed_at = Some(Utc::now());
                Ok(())
            }
            None => Err(DomainError::Conflict(
                "merge request is no longer open".to_string(),
            )),
        }
    }

    /// Substring match, not full-text.
    async fn search(
        &self,
        repository_ids: &[Uuid],
        query: &str,
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let query = query.to_lowercase();
        Ok(self
            .merge_requests
            .lock()
            .unwrap()
            .iter()
            .filter(|m| {
                repository_ids.contains(&m.repository_id)
                    && (m.title.to_lowercase().contains(&query)
                        || m.description.to_lowercase().contains(&query))
            })
            .take(row_limit(limit))
            .cloned()
            .collect())
    }

    async fn list_authored_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let mut mrs: Vec<MergeRequest> = filter_in(&self.merge_requests, |m| {
            m.author_id == Some(user_id)
                && m.status == MergeRequestStatus::Open
                && repository_ids.contains(&m.repository_id)
        });
        mrs.sort_by_key(|m| Reverse(m.created_at));
        mrs.truncate(row_limit(limit));
        Ok(mrs)
    }

    /// Any review by the user excludes the merge request whatever its `source_sha`: staleness only matters at merge time.
    async fn list_awaiting_review_by(
        &self,
        user_id: Uuid,
        repository_ids: &[Uuid],
        limit: i64,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        self.list_awaiting_review_by_calls
            .lock()
            .unwrap()
            .push((user_id, repository_ids.to_vec()));
        let reviewed_by_user: HashSet<Uuid> = self
            .reviews
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.user_id == user_id)
            .map(|r| r.merge_request_id)
            .collect();
        let mut mrs: Vec<MergeRequest> = filter_in(&self.merge_requests, |m| {
            m.status == MergeRequestStatus::Open
                && m.author_id != Some(user_id)
                && repository_ids.contains(&m.repository_id)
                && !reviewed_by_user.contains(&m.id)
        });
        mrs.sort_by_key(|m| Reverse(m.created_at));
        mrs.truncate(row_limit(limit));
        Ok(mrs)
    }
}

#[async_trait]
impl MergeRequestCommentPort for FakeMergeRequests {
    async fn add_comment(
        &self,
        new_comment: NewMergeRequestComment,
    ) -> Result<MergeRequestComment, DomainError> {
        let (file_path, line_number, end_line, side, anchor_content) = match new_comment.anchor {
            Some(a) => (
                Some(a.file_path),
                Some(a.line_number),
                a.end_line,
                Some(a.side),
                Some(a.anchor_content),
            ),
            None => (None, None, None, None, None),
        };
        let comment = MergeRequestComment {
            id: Uuid::new_v4(),
            merge_request_id: new_comment.merge_request_id,
            author_id: Some(new_comment.author_id),
            body: new_comment.body,
            created_at: Utc::now(),
            reply_to_id: new_comment.reply_to_id,
            file_path,
            line_number,
            side,
            anchor_content,
            resolved: false,
            end_line,
            suggested_content: new_comment.suggested_content,
            applied_at: None,
            applied_commit_sha: None,
        };
        self.comments.lock().unwrap().push(comment.clone());
        Ok(comment)
    }

    async fn list_comments(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<MergeRequestComment>, DomainError> {
        let mut comments: Vec<MergeRequestComment> =
            filter_in(&self.comments, |c| c.merge_request_id == merge_request_id);
        comments.sort_by_key(|c| c.created_at);
        Ok(comments)
    }

    async fn set_comment_resolved(
        &self,
        comment_id: Uuid,
        resolved: bool,
    ) -> Result<(), DomainError> {
        let mut comments = self.comments.lock().unwrap();
        let comment = comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .ok_or_else(|| DomainError::NotFound("comment".to_string()))?;
        comment.resolved = resolved;
        Ok(())
    }

    async fn mark_comment_applied(
        &self,
        comment_id: Uuid,
        commit_sha: &str,
    ) -> Result<MergeRequestComment, DomainError> {
        let mut comments = self.comments.lock().unwrap();
        let comment = comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .ok_or_else(|| DomainError::NotFound("comment".to_string()))?;
        comment.applied_at = Some(Utc::now());
        comment.applied_commit_sha = Some(commit_sha.to_string());
        Ok(comment.clone())
    }
}

#[async_trait]
impl MergeRequestReviewPort for FakeMergeRequests {
    /// `username` is always "someone" since there's no `UserRepositoryPort` here. Seed reviews with `with_reviews` for a
    /// specific one.
    async fn upsert_review(
        &self,
        merge_request_id: Uuid,
        user_id: Uuid,
        decision: ReviewDecision,
        source_sha: &str,
    ) -> Result<MergeRequestReview, DomainError> {
        let mut reviews = self.reviews.lock().unwrap();
        if let Some(existing) = reviews
            .iter_mut()
            .find(|r| r.merge_request_id == merge_request_id && r.user_id == user_id)
        {
            existing.decision = decision;
            existing.source_sha = source_sha.to_string();
            existing.created_at = Utc::now();
            return Ok(existing.clone());
        }
        let review = MergeRequestReview {
            merge_request_id,
            user_id,
            username: "someone".to_string(),
            decision,
            source_sha: source_sha.to_string(),
            created_at: Utc::now(),
        };
        reviews.push(review.clone());
        Ok(review)
    }

    async fn list_reviews(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<MergeRequestReview>, DomainError> {
        let mut reviews: Vec<MergeRequestReview> =
            filter_in(&self.reviews, |r| r.merge_request_id == merge_request_id);
        reviews.sort_by_key(|r| r.created_at);
        Ok(reviews)
    }
}

pub struct FakeLabels {
    labels: Mutex<Vec<Label>>,
    issue_links: Mutex<Vec<(Uuid, Uuid)>>,
    mr_links: Mutex<Vec<(Uuid, Uuid)>>,
}

impl FakeLabels {
    pub fn new(labels: Vec<Label>) -> Self {
        Self {
            labels: Mutex::new(labels),
            issue_links: Mutex::new(vec![]),
            mr_links: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Label> {
        find_in(&self.labels, |l| l.id == id)
    }

    pub fn snapshot(&self) -> Vec<Label> {
        self.labels.lock().unwrap().clone()
    }

    /// The labels linked to `owner_id`, by name.
    fn labels_of(&self, links: &Mutex<Vec<(Uuid, Uuid)>>, owner_id: Uuid) -> Vec<Label> {
        let mut labels: Vec<Label> = self
            .labels_of_each(links, &[owner_id])
            .into_iter()
            .map(|(_, label)| label)
            .collect();
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        labels
    }

    /// `(owner, label)` for every link whose owner is in `owner_ids`, in link order.
    fn labels_of_each(
        &self,
        links: &Mutex<Vec<(Uuid, Uuid)>>,
        owner_ids: &[Uuid],
    ) -> Vec<(Uuid, Label)> {
        let links = links.lock().unwrap();
        let labels = self.labels.lock().unwrap();
        links
            .iter()
            .filter(|(owner, _)| owner_ids.contains(owner))
            .filter_map(|(owner, label_id)| {
                labels
                    .iter()
                    .find(|l| l.id == *label_id)
                    .map(|l| (*owner, l.clone()))
            })
            .collect()
    }
}

#[async_trait]
impl LabelStorePort for FakeLabels {
    async fn create(&self, new_label: NewLabel) -> Result<Label, DomainError> {
        let label = Label {
            id: Uuid::new_v4(),
            name: new_label.name,
            color: new_label.color,
            repository_id: new_label.repository_id,
            group_id: new_label.group_id,
            created_at: Utc::now(),
        };
        self.labels.lock().unwrap().push(label.clone());
        Ok(label)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Label>, DomainError> {
        Ok(find_in(&self.labels, |l| l.id == id))
    }

    async fn update(&self, id: Uuid, name: String, color: String) -> Result<(), DomainError> {
        update_in(
            &self.labels,
            |l| l.id == id,
            |label| {
                label.name = name;
                label.color = color;
            },
        );
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.labels.lock().unwrap().retain(|l| l.id != id);
        self.issue_links
            .lock()
            .unwrap()
            .retain(|(_, label_id)| *label_id != id);
        self.mr_links
            .lock()
            .unwrap()
            .retain(|(_, label_id)| *label_id != id);
        Ok(())
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Label>, DomainError> {
        let mut labels: Vec<Label> =
            filter_in(&self.labels, |l| l.repository_id == Some(repository_id));
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(labels)
    }

    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Label>, DomainError> {
        let mut labels: Vec<Label> = filter_in(&self.labels, |l| l.group_id == Some(group_id));
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(labels)
    }

    async fn set_labels_for_issue(
        &self,
        issue_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), DomainError> {
        set_links(&self.issue_links, issue_id, label_ids);
        Ok(())
    }

    async fn list_for_issue(&self, issue_id: Uuid) -> Result<Vec<Label>, DomainError> {
        Ok(self.labels_of(&self.issue_links, issue_id))
    }

    async fn list_for_issues(&self, issue_ids: &[Uuid]) -> Result<Vec<(Uuid, Label)>, DomainError> {
        Ok(self.labels_of_each(&self.issue_links, issue_ids))
    }

    async fn set_labels_for_merge_request(
        &self,
        merge_request_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), DomainError> {
        set_links(&self.mr_links, merge_request_id, label_ids);
        Ok(())
    }

    async fn list_for_merge_request(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<Label>, DomainError> {
        Ok(self.labels_of(&self.mr_links, merge_request_id))
    }

    async fn list_for_merge_requests(
        &self,
        merge_request_ids: &[Uuid],
    ) -> Result<Vec<(Uuid, Label)>, DomainError> {
        Ok(self.labels_of_each(&self.mr_links, merge_request_ids))
    }
}

/// Replaces everything linked to `owner_id`. A repeated label id gives one link, like the join table's primary key.
fn set_links(links: &Mutex<Vec<(Uuid, Uuid)>>, owner_id: Uuid, label_ids: &[Uuid]) {
    let mut links = links.lock().unwrap();
    links.retain(|(owner, _)| *owner != owner_id);
    let mut seen = HashSet::new();
    links.extend(
        label_ids
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .map(|label_id| (owner_id, label_id)),
    );
}

pub struct FakeReleases {
    releases: Mutex<Vec<Release>>,
    assets: Mutex<Vec<ReleaseAsset>>,
    deleted_release_ids: Mutex<Vec<(Uuid, Uuid)>>,
    deleted_asset_ids: Mutex<Vec<(Uuid, Uuid)>>,
}

impl FakeReleases {
    pub fn new(releases: Vec<Release>) -> Self {
        Self {
            releases: Mutex::new(releases),
            assets: Mutex::new(vec![]),
            deleted_release_ids: Mutex::new(vec![]),
            deleted_asset_ids: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn with_assets(self, assets: Vec<ReleaseAsset>) -> Self {
        *self.assets.lock().unwrap() = assets;
        self
    }

    pub fn get(&self, id: Uuid) -> Option<Release> {
        find_in(&self.releases, |r| r.id == id)
    }

    pub fn snapshot(&self) -> Vec<Release> {
        self.releases.lock().unwrap().clone()
    }

    pub fn assets_snapshot(&self) -> Vec<ReleaseAsset> {
        self.assets.lock().unwrap().clone()
    }

    pub fn deleted_release_ids(&self) -> Vec<(Uuid, Uuid)> {
        self.deleted_release_ids.lock().unwrap().clone()
    }

    pub fn deleted_asset_ids(&self) -> Vec<(Uuid, Uuid)> {
        self.deleted_asset_ids.lock().unwrap().clone()
    }
}

#[async_trait]
impl ReleaseStorePort for FakeReleases {
    async fn create(&self, new_release: NewRelease) -> Result<Release, DomainError> {
        let mut releases = self.releases.lock().unwrap();
        if releases.iter().any(|r| {
            r.repository_id == new_release.repository_id && r.tag_name == new_release.tag_name
        }) {
            return Err(DomainError::Conflict(format!(
                "a release for tag '{}' already exists",
                new_release.tag_name
            )));
        }
        let release = Release {
            id: Uuid::new_v4(),
            repository_id: new_release.repository_id,
            tag_name: new_release.tag_name,
            title: new_release.title,
            notes: new_release.notes,
            draft: new_release.draft,
            prerelease: new_release.prerelease,
            author_id: Some(new_release.author_id),
            created_at: Utc::now(),
            published_at: if new_release.draft {
                None
            } else {
                Some(Utc::now())
            },
        };
        releases.push(release.clone());
        Ok(release)
    }

    async fn list_for_repository(
        &self,
        repository_id: Uuid,
        include_drafts: bool,
    ) -> Result<Vec<Release>, DomainError> {
        let mut releases: Vec<Release> = filter_in(&self.releases, |r| {
            r.repository_id == repository_id && (!r.draft || include_drafts)
        });
        releases.sort_by_key(|r| Reverse(r.published_at.unwrap_or(r.created_at)));
        Ok(releases)
    }

    async fn find_by_tag_name(
        &self,
        repository_id: Uuid,
        tag_name: &str,
    ) -> Result<Option<Release>, DomainError> {
        Ok(find_in(&self.releases, |r| {
            r.repository_id == repository_id && r.tag_name == tag_name
        }))
    }

    async fn update(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: ReleaseUpdate,
    ) -> Result<Release, DomainError> {
        let mut releases = self.releases.lock().unwrap();
        let release = releases
            .iter_mut()
            .find(|r| r.id == id && r.repository_id == repository_id)
            .ok_or_else(|| DomainError::NotFound("release".to_string()))?;
        if let Some(title) = update.title {
            release.title = title;
        }
        if let Some(notes) = update.notes {
            release.notes = notes;
        }
        if let Some(prerelease) = update.prerelease {
            release.prerelease = prerelease;
        }
        Ok(release.clone())
    }

    async fn publish(&self, id: Uuid, repository_id: Uuid) -> Result<Release, DomainError> {
        let mut releases = self.releases.lock().unwrap();
        let release = releases
            .iter_mut()
            .find(|r| r.id == id && r.repository_id == repository_id)
            .ok_or_else(|| DomainError::NotFound("release".to_string()))?;
        release.draft = false;
        release.published_at = release.published_at.or(Some(Utc::now()));
        Ok(release.clone())
    }

    async fn delete(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        let mut releases = self.releases.lock().unwrap();
        let before = releases.len();
        releases.retain(|r| !(r.id == id && r.repository_id == repository_id));
        if releases.len() == before {
            return Err(DomainError::NotFound("release".to_string()));
        }
        self.deleted_release_ids
            .lock()
            .unwrap()
            .push((id, repository_id));
        Ok(())
    }

    async fn create_asset(&self, new_asset: NewReleaseAsset) -> Result<ReleaseAsset, DomainError> {
        let asset = ReleaseAsset {
            id: new_asset.id,
            release_id: new_asset.release_id,
            filename: new_asset.filename,
            content_type: new_asset.content_type,
            size_bytes: new_asset.size_bytes,
            disk_path: new_asset.disk_path,
            uploaded_by: Some(new_asset.uploaded_by),
            created_at: Utc::now(),
        };
        self.assets.lock().unwrap().push(asset.clone());
        Ok(asset)
    }

    async fn list_assets(&self, release_id: Uuid) -> Result<Vec<ReleaseAsset>, DomainError> {
        let mut assets: Vec<ReleaseAsset> = filter_in(&self.assets, |a| a.release_id == release_id);
        assets.sort_by_key(|a| a.created_at);
        Ok(assets)
    }

    async fn find_asset(
        &self,
        id: Uuid,
        release_id: Uuid,
    ) -> Result<Option<ReleaseAsset>, DomainError> {
        Ok(find_in(&self.assets, |a| {
            a.id == id && a.release_id == release_id
        }))
    }

    async fn delete_asset(&self, id: Uuid, release_id: Uuid) -> Result<(), DomainError> {
        let mut assets = self.assets.lock().unwrap();
        let before = assets.len();
        assets.retain(|a| !(a.id == id && a.release_id == release_id));
        if assets.len() == before {
            return Err(DomainError::NotFound("release asset".to_string()));
        }
        self.deleted_asset_ids
            .lock()
            .unwrap()
            .push((id, release_id));
        Ok(())
    }
}

pub struct FakeStorage {
    writes: Mutex<Vec<(Uuid, Uuid, Uuid, String)>>,
    deleted: Mutex<Vec<String>>,
    deleted_for_repository: Mutex<Vec<Uuid>>,
    fail_deletes: bool,
}

impl FakeStorage {
    pub fn new() -> Self {
        Self {
            writes: Mutex::new(vec![]),
            deleted: Mutex::new(vec![]),
            deleted_for_repository: Mutex::new(vec![]),
            fail_deletes: false,
        }
    }

    /// Makes `delete` and `delete_all_for_repository` fail with an `Infrastructure` error.
    pub fn failing() -> Self {
        Self {
            fail_deletes: true,
            ..Self::new()
        }
    }

    pub fn writes(&self) -> Vec<(Uuid, Uuid, Uuid, String)> {
        self.writes.lock().unwrap().clone()
    }

    pub fn deleted(&self) -> Vec<String> {
        self.deleted.lock().unwrap().clone()
    }

    pub fn deleted_for_repository(&self) -> Vec<Uuid> {
        self.deleted_for_repository.lock().unwrap().clone()
    }
}

impl Default for FakeStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReleaseAssetStoragePort for FakeStorage {
    async fn write(
        &self,
        repository_id: Uuid,
        release_id: Uuid,
        asset_id: Uuid,
        sanitized_filename: &str,
        _data: bytes::Bytes,
    ) -> Result<String, DomainError> {
        self.writes.lock().unwrap().push((
            repository_id,
            release_id,
            asset_id,
            sanitized_filename.to_string(),
        ));
        Ok(format!(
            "release-assets/{repository_id}/{release_id}/{asset_id}-{sanitized_filename}"
        ))
    }

    fn absolute_path(&self, disk_path: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(disk_path)
    }

    async fn delete(&self, disk_path: &str) -> Result<(), DomainError> {
        if self.fail_deletes {
            return Err(DomainError::Infrastructure("disk error".to_string()));
        }
        self.deleted.lock().unwrap().push(disk_path.to_string());
        Ok(())
    }

    async fn delete_all_for_repository(&self, repository_id: Uuid) -> Result<(), DomainError> {
        if self.fail_deletes {
            return Err(DomainError::Infrastructure("disk error".to_string()));
        }
        self.deleted_for_repository
            .lock()
            .unwrap()
            .push(repository_id);
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeTagCreator {
    created: Mutex<Vec<(String, String, String)>>,
    deleted: Mutex<Vec<(String, String)>>,
}

impl FakeTagCreator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn created(&self) -> Vec<(String, String, String)> {
        self.created.lock().unwrap().clone()
    }

    pub fn deleted(&self) -> Vec<(String, String)> {
        self.deleted.lock().unwrap().clone()
    }
}

#[async_trait]
impl TagCreatorPort for FakeTagCreator {
    async fn create_tag(
        &self,
        repository_disk_path: &str,
        tag_name: &str,
        target_commit_sha: &str,
    ) -> Result<(), DomainError> {
        self.created.lock().unwrap().push((
            repository_disk_path.to_string(),
            tag_name.to_string(),
            target_commit_sha.to_string(),
        ));
        Ok(())
    }

    async fn delete_tag(
        &self,
        repository_disk_path: &str,
        tag_name: &str,
    ) -> Result<(), DomainError> {
        self.deleted
            .lock()
            .unwrap()
            .push((repository_disk_path.to_string(), tag_name.to_string()));
        Ok(())
    }
}

pub struct FakeJobs {
    jobs: Mutex<Vec<Job>>,
}

impl FakeJobs {
    pub fn new(jobs: Vec<Job>) -> Self {
        Self {
            jobs: Mutex::new(jobs),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Job> {
        find_in(&self.jobs, |j| j.id == id)
    }

    pub fn snapshot(&self) -> Vec<Job> {
        self.jobs.lock().unwrap().clone()
    }
}

#[async_trait]
impl JobStorePort for FakeJobs {
    async fn create(&self, new_job: NewJob) -> Result<Job, DomainError> {
        let job = Job {
            id: Uuid::new_v4(),
            pipeline_id: new_job.pipeline_id,
            stage: new_job.stage,
            name: new_job.name,
            image: new_job.image,
            script: new_job.script,
            variables: new_job.variables,
            needs: new_job.needs,
            tags: new_job.tags,
            cache: new_job.cache,
            status: JobStatus::Pending,
            runner_id: None,
            logs: String::new(),
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        };
        self.jobs.lock().unwrap().push(job.clone());
        Ok(job)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Job>, DomainError> {
        Ok(find_in(&self.jobs, |j| j.id == id))
    }

    async fn list_for_pipeline(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
        let mut jobs: Vec<Job> = filter_in(&self.jobs, |j| j.pipeline_id == pipeline_id);
        jobs.sort_by_key(|j| j.created_at);
        Ok(jobs)
    }

    /// Claims the earliest pending job whose tags fit the runner and that `runnable_jobs` releases in its pipeline.
    async fn claim_next(
        &self,
        runner_id: Uuid,
        runner_tags: &[String],
    ) -> Result<Option<Job>, DomainError> {
        let mut jobs = self.jobs.lock().unwrap();
        let snapshot = jobs.clone();
        let mut pipeline_ids: Vec<Uuid> = snapshot.iter().map(|j| j.pipeline_id).collect();
        pipeline_ids.sort();
        pipeline_ids.dedup();
        let mut candidates: Vec<Job> = Vec::new();
        for pipeline_id in pipeline_ids {
            let mut siblings: Vec<Job> = snapshot
                .iter()
                .filter(|j| j.pipeline_id == pipeline_id)
                .cloned()
                .collect();
            siblings.sort_by_key(|j| j.created_at);
            candidates.extend(
                runnable_jobs(&siblings)
                    .into_iter()
                    .filter(|job| job.tags.iter().all(|t| runner_tags.contains(t)))
                    .cloned(),
            );
        }
        candidates.sort_by_key(|j| j.created_at);
        let Some(mut claimed) = candidates.into_iter().next() else {
            return Ok(None);
        };
        claimed.status = JobStatus::Running;
        claimed.runner_id = Some(runner_id);
        if let Some(stored) = jobs.iter_mut().find(|j| j.id == claimed.id) {
            *stored = claimed.clone();
        }
        Ok(Some(claimed))
    }

    async fn append_logs(&self, id: Uuid, chunk: &str) -> Result<(), DomainError> {
        update_in(
            &self.jobs,
            |j| j.id == id,
            |job| {
                job.logs.push_str(chunk);
            },
        );
        Ok(())
    }

    /// Terminal statuses are final: a job already in one is left alone and this returns `false`, as for an unknown id.
    async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<bool, DomainError> {
        let mut jobs = self.jobs.lock().unwrap();
        let Some(job) = jobs.iter_mut().find(|j| j.id == id) else {
            return Ok(false);
        };
        if job.status.is_terminal() {
            return Ok(false);
        }
        job.status = status;
        Ok(true)
    }

    async fn release_jobs_claimed_by(&self, runner_id: Uuid) -> Result<u64, DomainError> {
        let mut released = 0;
        for job in self.jobs.lock().unwrap().iter_mut() {
            if job.runner_id == Some(runner_id) && job.status == JobStatus::Running {
                job.status = JobStatus::Pending;
                job.runner_id = None;
                released += 1;
            }
        }
        Ok(released)
    }

    async fn count_running(&self) -> Result<i64, DomainError> {
        Ok(self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|j| j.status == JobStatus::Running)
            .count() as i64)
    }

    async fn list_runnable(&self, pipeline_id: Uuid) -> Result<Vec<Job>, DomainError> {
        let jobs = self.list_for_pipeline(pipeline_id).await?;
        Ok(runnable_jobs(&jobs).into_iter().cloned().collect())
    }
}

pub struct FakeMilestones {
    milestones: Mutex<Vec<Milestone>>,
}

impl FakeMilestones {
    pub fn new(milestones: Vec<Milestone>) -> Self {
        Self {
            milestones: Mutex::new(milestones),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Milestone> {
        find_in(&self.milestones, |m| m.id == id)
    }

    pub fn snapshot(&self) -> Vec<Milestone> {
        self.milestones.lock().unwrap().clone()
    }
}

#[async_trait]
impl MilestoneStorePort for FakeMilestones {
    async fn create(&self, new_milestone: NewMilestone) -> Result<Milestone, DomainError> {
        let milestone = Milestone {
            id: Uuid::new_v4(),
            title: new_milestone.title,
            description: new_milestone.description,
            due_date: new_milestone.due_date,
            state: MilestoneState::Open,
            repository_id: new_milestone.repository_id,
            group_id: new_milestone.group_id,
            created_at: Utc::now(),
        };
        self.milestones.lock().unwrap().push(milestone.clone());
        Ok(milestone)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Milestone>, DomainError> {
        Ok(find_in(&self.milestones, |m| m.id == id))
    }

    async fn update(
        &self,
        id: Uuid,
        title: String,
        description: String,
        due_date: Option<DateTime<Utc>>,
        state: MilestoneState,
    ) -> Result<(), DomainError> {
        update_in(
            &self.milestones,
            |m| m.id == id,
            |milestone| {
                milestone.title = title;
                milestone.description = description;
                milestone.due_date = due_date;
                milestone.state = state;
            },
        );
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.milestones.lock().unwrap().retain(|m| m.id != id);
        Ok(())
    }

    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<Milestone>, DomainError> {
        let mut milestones: Vec<Milestone> =
            filter_in(&self.milestones, |m| m.repository_id == Some(repository_id));
        milestones.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(milestones)
    }

    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Milestone>, DomainError> {
        let mut milestones: Vec<Milestone> =
            filter_in(&self.milestones, |m| m.group_id == Some(group_id));
        milestones.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(milestones)
    }
}

pub struct FakeCollaborators {
    rows: Mutex<Vec<(Uuid, Uuid, CollaboratorRole)>>,
}

impl FakeCollaborators {
    pub fn new(rows: Vec<(Uuid, Uuid, CollaboratorRole)>) -> Self {
        Self {
            rows: Mutex::new(rows),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn snapshot(&self) -> Vec<(Uuid, Uuid, CollaboratorRole)> {
        self.rows.lock().unwrap().clone()
    }
}

#[async_trait]
impl RepositoryCollaboratorStorePort for FakeCollaborators {
    async fn add(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        let mut rows = self.rows.lock().unwrap();
        if rows
            .iter()
            .any(|(r, u, _)| *r == repository_id && *u == user_id)
        {
            return Err(DomainError::Conflict("already a collaborator".to_string()));
        }
        rows.push((repository_id, user_id, role));
        Ok(())
    }

    async fn set_role(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
        role: CollaboratorRole,
    ) -> Result<(), DomainError> {
        let mut rows = self.rows.lock().unwrap();
        let Some(row) = rows
            .iter_mut()
            .find(|(r, u, _)| *r == repository_id && *u == user_id)
        else {
            return Err(DomainError::NotFound("collaborator".to_string()));
        };
        row.2 = role;
        Ok(())
    }

    /// Doesn't fail if the pair doesn't exist.
    async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        self.rows
            .lock()
            .unwrap()
            .retain(|(r, u, _)| !(*r == repository_id && *u == user_id));
        Ok(())
    }

    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<RepositoryCollaborator>, DomainError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|(r, _, _)| *r == repository_id)
            .map(|(r, u, role)| RepositoryCollaborator {
                repository_id: *r,
                user_id: *u,
                username: "someone".to_string(),
                role: *role,
                created_at: Utc::now(),
            })
            .collect())
    }

    async fn get_role(
        &self,
        repository_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|(r, u, _)| *r == repository_id && *u == user_id)
            .map(|(_, _, role)| *role))
    }

    async fn list_repositories_for_collaborator(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, DomainError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, u, _)| *u == user_id)
            .map(|(r, _, _)| *r)
            .collect())
    }
}

/// Keeps webhook secrets in plaintext, only to show the right secret was passed through.
pub struct FakeWebhookStore {
    webhooks: Mutex<Vec<Webhook>>,
    secrets: Mutex<HashMap<Uuid, String>>,
    deliveries: Mutex<Vec<WebhookDelivery>>,
}

impl FakeWebhookStore {
    pub fn new(webhooks: Vec<Webhook>) -> Self {
        Self {
            webhooks: Mutex::new(webhooks),
            secrets: Mutex::new(HashMap::new()),
            deliveries: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Webhook> {
        find_in(&self.webhooks, |w| w.id == id)
    }

    pub fn snapshot(&self) -> Vec<Webhook> {
        self.webhooks.lock().unwrap().clone()
    }
}

impl Default for FakeWebhookStore {
    fn default() -> Self {
        Self::empty()
    }
}

#[async_trait]
impl WebhookStorePort for FakeWebhookStore {
    async fn create(&self, new_webhook: NewWebhook) -> Result<Webhook, DomainError> {
        let webhook = Webhook {
            id: Uuid::new_v4(),
            repository_id: new_webhook.repository_id,
            url: new_webhook.url,
            events: new_webhook.events,
            active: true,
            created_at: Utc::now(),
        };
        self.secrets
            .lock()
            .unwrap()
            .insert(webhook.id, new_webhook.secret_plaintext);
        self.webhooks.lock().unwrap().push(webhook.clone());
        Ok(webhook)
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Webhook>, DomainError> {
        Ok(filter_in(&self.webhooks, |w| {
            w.repository_id == repository_id
        }))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, DomainError> {
        Ok(find_in(&self.webhooks, |w| w.id == id))
    }

    async fn update(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: WebhookUpdate,
    ) -> Result<Webhook, DomainError> {
        let mut webhooks = self.webhooks.lock().unwrap();
        let webhook = webhooks
            .iter_mut()
            .find(|w| w.id == id && w.repository_id == repository_id)
            .ok_or_else(|| DomainError::NotFound("webhook".to_string()))?;
        if let Some(url) = update.url {
            webhook.url = url;
        }
        if let Some(secret_plaintext) = update.secret_plaintext {
            self.secrets.lock().unwrap().insert(id, secret_plaintext);
        }
        if let Some(events) = update.events {
            webhook.events = events;
        }
        if let Some(active) = update.active {
            webhook.active = active;
        }
        Ok(webhook.clone())
    }

    async fn delete(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        self.webhooks
            .lock()
            .unwrap()
            .retain(|w| !(w.id == id && w.repository_id == repository_id));
        self.secrets.lock().unwrap().remove(&id);
        Ok(())
    }

    async fn list_active_for_event(
        &self,
        repository_id: Uuid,
        event_kind: &str,
    ) -> Result<Vec<Webhook>, DomainError> {
        Ok(filter_in(&self.webhooks, |w| {
            w.repository_id == repository_id && w.active && w.events.iter().any(|e| e == event_kind)
        }))
    }

    async fn resolve_secret_plaintext(&self, webhook_id: Uuid) -> Result<String, DomainError> {
        self.secrets
            .lock()
            .unwrap()
            .get(&webhook_id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound("webhook".to_string()))
    }

    async fn record_delivery(&self, delivery: NewWebhookDelivery) -> Result<(), DomainError> {
        self.deliveries.lock().unwrap().push(WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id: delivery.webhook_id,
            event_kind: delivery.event_kind,
            http_status: delivery.http_status,
            success: delivery.success,
            error_message: delivery.error_message,
            created_at: Utc::now(),
        });
        Ok(())
    }

    async fn list_deliveries(
        &self,
        webhook_id: Uuid,
        repository_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DomainError> {
        if !self
            .webhooks
            .lock()
            .unwrap()
            .iter()
            .any(|w| w.id == webhook_id && w.repository_id == repository_id)
        {
            return Err(DomainError::NotFound("webhook".to_string()));
        }
        let mut deliveries: Vec<WebhookDelivery> =
            filter_in(&self.deliveries, |d| d.webhook_id == webhook_id);
        deliveries.sort_by_key(|d| Reverse(d.created_at));
        deliveries.truncate(row_limit(limit));
        Ok(deliveries)
    }
}

pub struct FakeDiffReader {
    diffs: Vec<FileDiff>,
}

impl FakeDiffReader {
    pub fn new(diffs: Vec<FileDiff>) -> Self {
        Self { diffs }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }
}

#[async_trait]
impl DiffReaderPort for FakeDiffReader {
    async fn diff_branches(
        &self,
        _repository_disk_path: &str,
        _source_branch: &str,
        _target_branch: &str,
    ) -> Result<Vec<FileDiff>, DomainError> {
        Ok(self.diffs.clone())
    }
}

pub struct FakeBranchReader {
    branches: Vec<BranchInfo>,
}

impl FakeBranchReader {
    pub fn new(branches: Vec<BranchInfo>) -> Self {
        Self { branches }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }
}

#[async_trait]
impl BranchReaderPort for FakeBranchReader {
    async fn list_branches(
        &self,
        _repository_disk_path: &str,
    ) -> Result<Vec<BranchInfo>, DomainError> {
        Ok(self.branches.clone())
    }
}

pub struct FakeWikis {
    wikis: Mutex<Vec<Wiki>>,
}

impl FakeWikis {
    pub fn new(wikis: Vec<Wiki>) -> Self {
        Self {
            wikis: Mutex::new(wikis),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, repository_id: Uuid) -> Option<Wiki> {
        find_in(&self.wikis, |w| w.repository_id == repository_id)
    }
}

#[async_trait]
impl WikiStorePort for FakeWikis {
    async fn find_by_repository_id(
        &self,
        repository_id: Uuid,
    ) -> Result<Option<Wiki>, DomainError> {
        Ok(find_in(&self.wikis, |w| w.repository_id == repository_id))
    }

    /// An existing row wins: `new_wiki.disk_path` only counts the first time, like `ON CONFLICT`.
    async fn find_or_create(&self, new_wiki: NewWiki) -> Result<Wiki, DomainError> {
        let mut wikis = self.wikis.lock().unwrap();
        if let Some(existing) = wikis
            .iter()
            .find(|w| w.repository_id == new_wiki.repository_id)
        {
            return Ok(existing.clone());
        }
        let wiki = Wiki {
            id: Uuid::new_v4(),
            repository_id: new_wiki.repository_id,
            disk_path: new_wiki.disk_path,
            created_at: Utc::now(),
        };
        wikis.push(wiki.clone());
        Ok(wiki)
    }
}

/// Tracks a head sha and the pages per `wiki_disk_path`, with the same compare-and-swap as `GitWikiWriter`: `base_sha`
/// must equal the head (`None` only for the first commit), else `Conflict`.
#[derive(Default)]
pub struct FakeWikiWriter {
    ensure_calls: Mutex<Vec<String>>,
    save_calls: Mutex<Vec<(String, String, Option<String>)>>,
    delete_calls: Mutex<Vec<(String, String, String)>>,
    heads: Mutex<HashMap<String, String>>,
    pages: Mutex<HashMap<(String, String), String>>,
    next_sha: Mutex<u64>,
}

impl FakeWikiWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ensure_calls(&self) -> Vec<String> {
        self.ensure_calls.lock().unwrap().clone()
    }

    pub fn save_calls(&self) -> Vec<(String, String, Option<String>)> {
        self.save_calls.lock().unwrap().clone()
    }

    pub fn delete_calls(&self) -> Vec<(String, String, String)> {
        self.delete_calls.lock().unwrap().clone()
    }

    pub fn page_content_of(&self, wiki_disk_path: &str, slug: &str) -> Option<String> {
        self.pages
            .lock()
            .unwrap()
            .get(&(wiki_disk_path.to_string(), slug.to_string()))
            .cloned()
    }

    /// A unique 40-hex sha per write, so writes never collide on a `base_sha`.
    fn fresh_sha(&self) -> String {
        let mut next = self.next_sha.lock().unwrap();
        *next += 1;
        format!("{:040x}", *next)
    }
}

#[async_trait]
impl WikiWriterPort for FakeWikiWriter {
    async fn ensure_wiki_repo_exists(&self, wiki_disk_path: &str) -> Result<(), DomainError> {
        self.ensure_calls
            .lock()
            .unwrap()
            .push(wiki_disk_path.to_string());
        Ok(())
    }

    /// Compare-and-swap on the in-memory head like `GitWikiWriter::save_page`: `Conflict` unless `base_sha` is the current
    /// head (`None` for a wiki with no commits).
    async fn save_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        content: &str,
        base_sha: Option<&str>,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<WikiRevision, DomainError> {
        self.save_calls.lock().unwrap().push((
            wiki_disk_path.to_string(),
            slug.to_string(),
            base_sha.map(str::to_string),
        ));
        let mut heads = self.heads.lock().unwrap();
        if heads.get(wiki_disk_path).map(String::as_str) != base_sha {
            return Err(DomainError::Conflict(
                "the wiki changed since you loaded this page".to_string(),
            ));
        }
        let new_sha = self.fresh_sha();
        heads.insert(wiki_disk_path.to_string(), new_sha.clone());
        drop(heads);
        self.pages.lock().unwrap().insert(
            (wiki_disk_path.to_string(), slug.to_string()),
            content.to_string(),
        );
        Ok(WikiRevision {
            commit_sha: new_sha,
            author_name: author_name.to_string(),
            author_email: author_email.to_string(),
            committed_at: Utc::now(),
            message: message.to_string(),
        })
    }

    /// Same compare-and-swap as `save_page`, plus `NotFound` when the page isn't stored.
    async fn delete_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        base_sha: &str,
        _author_name: &str,
        _author_email: &str,
        _message: &str,
    ) -> Result<(), DomainError> {
        self.delete_calls.lock().unwrap().push((
            wiki_disk_path.to_string(),
            slug.to_string(),
            base_sha.to_string(),
        ));
        let mut heads = self.heads.lock().unwrap();
        if heads.get(wiki_disk_path).map(String::as_str) != Some(base_sha) {
            return Err(DomainError::Conflict(
                "the wiki changed since you loaded this page".to_string(),
            ));
        }
        let mut pages = self.pages.lock().unwrap();
        if pages
            .remove(&(wiki_disk_path.to_string(), slug.to_string()))
            .is_none()
        {
            return Err(DomainError::NotFound(format!("wiki page '{slug}'")));
        }
        drop(pages);
        let new_sha = self.fresh_sha();
        heads.insert(wiki_disk_path.to_string(), new_sha);
        Ok(())
    }

    async fn heal_dangling_head(&self, _wiki_disk_path: &str) -> Result<(), DomainError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeEvents {
    security: Mutex<Vec<(SecurityEvent, Option<Uuid>)>>,
    pipeline: Mutex<Vec<(Uuid, PipelineEvent)>>,
    job: Mutex<Vec<(Uuid, JobEvent)>>,
}

impl FakeEvents {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn security_events(&self) -> Vec<(SecurityEvent, Option<Uuid>)> {
        self.security.lock().unwrap().clone()
    }

    pub fn pipeline_events(&self) -> Vec<(Uuid, PipelineEvent)> {
        self.pipeline.lock().unwrap().clone()
    }

    pub fn job_events(&self) -> Vec<(Uuid, JobEvent)> {
        self.job.lock().unwrap().clone()
    }
}

#[async_trait]
impl EventPublisherPort for FakeEvents {
    async fn publish_security_event(
        &self,
        event: SecurityEvent,
        actor_id: Option<Uuid>,
    ) -> Result<(), DomainError> {
        self.security.lock().unwrap().push((event, actor_id));
        Ok(())
    }
}

#[async_trait]
impl PipelineEventPublisherPort for FakeEvents {
    async fn publish_pipeline_event(
        &self,
        pipeline_id: Uuid,
        event: PipelineEvent,
    ) -> Result<(), DomainError> {
        self.pipeline.lock().unwrap().push((pipeline_id, event));
        Ok(())
    }

    async fn publish_job_event(&self, job_id: Uuid, event: JobEvent) -> Result<(), DomainError> {
        self.job.lock().unwrap().push((job_id, event));
        Ok(())
    }
}

/// `update_status` sets unconditionally, the terminal-status guards live in the use cases.
pub struct FakePipelines {
    pipelines: Mutex<Vec<Pipeline>>,
}

impl FakePipelines {
    pub fn new(pipelines: Vec<Pipeline>) -> Self {
        Self {
            pipelines: Mutex::new(pipelines),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn get(&self, id: Uuid) -> Option<Pipeline> {
        find_in(&self.pipelines, |p| p.id == id)
    }

    pub fn snapshot(&self) -> Vec<Pipeline> {
        self.pipelines.lock().unwrap().clone()
    }
}

impl Default for FakePipelines {
    fn default() -> Self {
        Self::empty()
    }
}

#[async_trait]
impl PipelineStorePort for FakePipelines {
    async fn create(&self, new_pipeline: NewPipeline) -> Result<Pipeline, DomainError> {
        let pipeline = Pipeline {
            id: Uuid::new_v4(),
            repository_id: new_pipeline.repository_id,
            commit_sha: new_pipeline.commit_sha,
            execution_engine: new_pipeline.execution_engine,
            status: PipelineStatus::Pending,
            triggered_by: new_pipeline.triggered_by,
            created_at: Utc::now(),
            finished_at: None,
            error: None,
        };
        self.pipelines.lock().unwrap().push(pipeline.clone());
        Ok(pipeline)
    }

    async fn create_failed(
        &self,
        new_pipeline: NewPipeline,
        error: &str,
    ) -> Result<Pipeline, DomainError> {
        let mut pipeline = self.create(new_pipeline).await?;
        pipeline.status = PipelineStatus::Failed;
        pipeline.finished_at = Some(Utc::now());
        pipeline.error = Some(error.to_string());
        update_in(
            &self.pipelines,
            |p| p.id == pipeline.id,
            |stored| {
                *stored = pipeline.clone();
            },
        );
        Ok(pipeline)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Pipeline>, DomainError> {
        Ok(find_in(&self.pipelines, |p| p.id == id))
    }

    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Pipeline>, DomainError> {
        let mut pipelines: Vec<Pipeline> =
            filter_in(&self.pipelines, |p| p.repository_id == repository_id);
        pipelines.sort_by_key(|p| Reverse(p.created_at));
        Ok(pipelines)
    }

    async fn update_status(&self, id: Uuid, status: PipelineStatus) -> Result<(), DomainError> {
        update_in(
            &self.pipelines,
            |p| p.id == id,
            |pipeline| pipeline.status = status,
        );
        Ok(())
    }

    async fn mark_running(&self, id: Uuid) -> Result<bool, DomainError> {
        let mut pipelines = self.pipelines.lock().unwrap();
        let Some(pipeline) = pipelines
            .iter_mut()
            .find(|p| p.id == id && p.status == PipelineStatus::Pending)
        else {
            return Ok(false);
        };
        pipeline.status = PipelineStatus::Running;
        Ok(true)
    }

    async fn count_created_since(&self, since: DateTime<Utc>) -> Result<i64, DomainError> {
        Ok(self
            .pipelines
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.created_at >= since)
            .count() as i64)
    }
}

/// Records `submit` and `cancel` in separate lists so asserting on one can't be satisfied by a call to the other.
#[derive(Default)]
pub struct FakeExecution {
    submitted: Mutex<Vec<Uuid>>,
    canceled: Mutex<Vec<Uuid>>,
}

impl FakeExecution {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submitted(&self) -> Vec<Uuid> {
        self.submitted.lock().unwrap().clone()
    }

    pub fn canceled(&self) -> Vec<Uuid> {
        self.canceled.lock().unwrap().clone()
    }
}

#[async_trait]
impl JobExecutionPort for FakeExecution {
    async fn submit(&self, job: &Job) -> Result<(), DomainError> {
        self.submitted.lock().unwrap().push(job.id);
        Ok(())
    }

    async fn cancel(&self, job: &Job) -> Result<(), DomainError> {
        self.canceled.lock().unwrap().push(job.id);
        Ok(())
    }
}

/// Holds one `RepositorySettings` and one CI variable set. Rows come back with the `repository_id` that was asked for,
/// not the one the fake was built with.
pub struct FakeRepositorySettings {
    settings: Mutex<RepositorySettings>,
    ci_variables: Mutex<Vec<(CiVariable, String)>>,
}

impl FakeRepositorySettings {
    pub fn new(settings: RepositorySettings) -> Self {
        Self {
            settings: Mutex::new(settings),
            ci_variables: Mutex::new(vec![]),
        }
    }
}

#[async_trait]
impl RepositorySettingsStorePort for FakeRepositorySettings {
    async fn get_or_create_default(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositorySettings, DomainError> {
        Ok(RepositorySettings {
            repository_id,
            ..self.settings.lock().unwrap().clone()
        })
    }

    async fn update(
        &self,
        repository_id: Uuid,
        update: RepositorySettingsUpdate,
    ) -> Result<RepositorySettings, DomainError> {
        let mut settings = self.settings.lock().unwrap();
        if let Some(pipeline_file_path) = update.pipeline_file_path {
            settings.pipeline_file_path = pipeline_file_path;
        }
        if let Some(ci_enabled) = update.ci_enabled {
            settings.ci_enabled = ci_enabled;
        }
        if let Some(required_approvals) = update.required_approvals {
            settings.required_approvals = required_approvals;
        }
        Ok(RepositorySettings {
            repository_id,
            ..settings.clone()
        })
    }

    async fn list_ci_variables(&self, repository_id: Uuid) -> Result<Vec<CiVariable>, DomainError> {
        let mut variables: Vec<CiVariable> = self
            .ci_variables
            .lock()
            .unwrap()
            .iter()
            .filter(|(v, _)| v.repository_id == repository_id)
            .map(|(v, _)| v.clone())
            .collect();
        variables.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(variables)
    }

    async fn set_ci_variable(
        &self,
        new_variable: NewCiVariable,
    ) -> Result<CiVariable, DomainError> {
        let mut variables = self.ci_variables.lock().unwrap();
        if let Some(existing) = variables.iter_mut().find(|(v, _)| {
            v.repository_id == new_variable.repository_id && v.key == new_variable.key
        }) {
            existing.0.masked = new_variable.masked;
            existing.1 = new_variable.plaintext_value;
            return Ok(existing.0.clone());
        }
        let variable = CiVariable {
            id: Uuid::new_v4(),
            repository_id: new_variable.repository_id,
            key: new_variable.key,
            masked: new_variable.masked,
        };
        variables.push((variable.clone(), new_variable.plaintext_value));
        Ok(variable)
    }

    async fn delete_ci_variable(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        let mut variables = self.ci_variables.lock().unwrap();
        let before = variables.len();
        variables.retain(|(v, _)| !(v.id == id && v.repository_id == repository_id));
        if variables.len() == before {
            return Err(DomainError::NotFound("ci variable".to_string()));
        }
        Ok(())
    }

    async fn resolve_ci_variables_plaintext(
        &self,
        repository_id: Uuid,
    ) -> Result<BTreeMap<String, String>, DomainError> {
        Ok(self
            .ci_variables
            .lock()
            .unwrap()
            .iter()
            .filter(|(v, _)| v.repository_id == repository_id)
            .map(|(v, plaintext)| (v.key.clone(), plaintext.clone()))
            .collect())
    }
}

pub struct FakeFileReader(Option<Vec<u8>>);

impl FakeFileReader {
    pub fn new(content: Option<Vec<u8>>) -> Self {
        Self(content)
    }

    pub fn none() -> Self {
        Self(None)
    }
}

#[async_trait]
impl PipelineFileReaderPort for FakeFileReader {
    async fn read_file_at_revision(
        &self,
        _repository_disk_path: &str,
        _revision: &str,
        _path: &str,
    ) -> Result<Option<Vec<u8>>, DomainError> {
        Ok(self.0.clone())
    }
}

/// `update` applies every field and keeps the `None` / `Some(None)` / `Some(Some(v))` distinction of the nested options.
pub struct FakeSystemSettings(Mutex<SystemSettings>);

impl FakeSystemSettings {
    pub fn new(settings: SystemSettings) -> Self {
        Self(Mutex::new(settings))
    }
}

impl Default for FakeSystemSettings {
    fn default() -> Self {
        Self::new(SystemSettings {
            execution_engine: ExecutionEngine::DockerRunners,
            k8s_namespace: None,
            k8s_cache_storage_class: None,
            runner_registration_token: None,
            log_retention_days: None,
            max_concurrent_jobs: None,
            jwt_ttl_hours: 12,
            max_push_size_mb: 500,
        })
    }
}

#[async_trait]
impl SystemSettingsStorePort for FakeSystemSettings {
    async fn get(&self) -> Result<SystemSettings, DomainError> {
        Ok(self.0.lock().unwrap().clone())
    }

    async fn update(&self, update: SystemSettingsUpdate) -> Result<SystemSettings, DomainError> {
        let mut settings = self.0.lock().unwrap();
        if let Some(execution_engine) = update.execution_engine {
            settings.execution_engine = execution_engine;
        }
        if let Some(k8s_namespace) = update.k8s_namespace {
            settings.k8s_namespace = k8s_namespace;
        }
        if let Some(k8s_cache_storage_class) = update.k8s_cache_storage_class {
            settings.k8s_cache_storage_class = k8s_cache_storage_class;
        }
        if let Some(runner_registration_token) = update.runner_registration_token {
            settings.runner_registration_token = runner_registration_token;
        }
        if let Some(log_retention_days) = update.log_retention_days {
            settings.log_retention_days = log_retention_days;
        }
        if let Some(max_concurrent_jobs) = update.max_concurrent_jobs {
            settings.max_concurrent_jobs = max_concurrent_jobs;
        }
        if let Some(jwt_ttl_hours) = update.jwt_ttl_hours {
            settings.jwt_ttl_hours = jwt_ttl_hours;
        }
        if let Some(max_push_size_mb) = update.max_push_size_mb {
            settings.max_push_size_mb = max_push_size_mb;
        }
        Ok(settings.clone())
    }
}

/// Deleting an unknown id isn't an error, like the real adapter.
pub struct FakeRunners {
    runners: Mutex<Vec<Runner>>,
    deleted: Mutex<Vec<Uuid>>,
}

impl FakeRunners {
    pub fn new(runners: Vec<Runner>) -> Self {
        Self {
            runners: Mutex::new(runners),
            deleted: Mutex::new(vec![]),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn snapshot(&self) -> Vec<Runner> {
        self.runners.lock().unwrap().clone()
    }

    pub fn deleted_ids(&self) -> Vec<Uuid> {
        self.deleted.lock().unwrap().clone()
    }
}

impl Default for FakeRunners {
    fn default() -> Self {
        Self::empty()
    }
}

#[async_trait]
impl RunnerRepositoryPort for FakeRunners {
    async fn create(&self, new_runner: NewRunner) -> Result<Runner, DomainError> {
        let runner = Runner {
            id: Uuid::new_v4(),
            name: new_runner.name,
            token_hash: new_runner.token_hash,
            tags: new_runner.tags,
            last_heartbeat_at: None,
            created_at: Utc::now(),
        };
        self.runners.lock().unwrap().push(runner.clone());
        Ok(runner)
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Runner>, DomainError> {
        Ok(find_in(&self.runners, |r| r.token_hash == token_hash))
    }

    async fn list(&self) -> Result<Vec<Runner>, DomainError> {
        let mut runners = self.runners.lock().unwrap().clone();
        runners.sort_by_key(|r| Reverse(r.created_at));
        Ok(runners)
    }

    async fn touch_heartbeat(&self, id: Uuid) -> Result<(), DomainError> {
        update_in(
            &self.runners,
            |r| r.id == id,
            |runner| runner.last_heartbeat_at = Some(Utc::now()),
        );
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.runners.lock().unwrap().retain(|r| r.id != id);
        self.deleted.lock().unwrap().push(id);
        Ok(())
    }
}

pub struct FakeApiTokens {
    tokens: Mutex<Vec<ApiToken>>,
}

impl FakeApiTokens {
    pub fn new(tokens: Vec<ApiToken>) -> Self {
        Self {
            tokens: Mutex::new(tokens),
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn snapshot(&self) -> Vec<ApiToken> {
        self.tokens.lock().unwrap().clone()
    }
}

impl Default for FakeApiTokens {
    fn default() -> Self {
        Self::empty()
    }
}

#[async_trait]
impl ApiTokenRepositoryPort for FakeApiTokens {
    async fn create(&self, new_token: NewApiToken) -> Result<ApiToken, DomainError> {
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: new_token.user_id,
            name: new_token.name,
            token_hash: new_token.token_hash,
            created_at: Utc::now(),
            last_used_at: None,
        };
        self.tokens.lock().unwrap().push(token.clone());
        Ok(token)
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<ApiToken>, DomainError> {
        let mut tokens: Vec<ApiToken> = filter_in(&self.tokens, |t| t.user_id == user_id);
        tokens.sort_by_key(|t| Reverse(t.created_at));
        Ok(tokens)
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<ApiToken>, DomainError> {
        Ok(find_in(&self.tokens, |t| t.token_hash == token_hash))
    }

    async fn revoke(&self, id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        let mut tokens = self.tokens.lock().unwrap();
        let before = tokens.len();
        tokens.retain(|t| !(t.id == id && t.user_id == user_id));
        if tokens.len() == before {
            return Err(DomainError::NotFound("api token".to_string()));
        }
        Ok(())
    }

    async fn touch_last_used(&self, id: Uuid) -> Result<(), DomainError> {
        update_in(
            &self.tokens,
            |t| t.id == id,
            |token| token.last_used_at = Some(Utc::now()),
        );
        Ok(())
    }
}

pub struct FakeHasher;

impl PasswordHasherPort for FakeHasher {
    fn hash(&self, plain: &str) -> Result<String, DomainError> {
        Ok(format!("hashed:{plain}"))
    }

    fn verify(&self, plain: &str, hash: &str) -> Result<bool, DomainError> {
        Ok(hash == format!("hashed:{plain}"))
    }
}

/// A `FakeHasher` that records the thread each `hash` ran on, to check hashing stays off the async runtime's threads.
#[derive(Default)]
pub struct ThreadRecordingHasher {
    threads: Mutex<Vec<std::thread::ThreadId>>,
}

impl ThreadRecordingHasher {
    pub fn hash_threads(&self) -> Vec<std::thread::ThreadId> {
        self.threads.lock().unwrap().clone()
    }
}

impl PasswordHasherPort for ThreadRecordingHasher {
    fn hash(&self, plain: &str) -> Result<String, DomainError> {
        self.threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        Ok(format!("hashed:{plain}"))
    }

    fn verify(&self, plain: &str, hash: &str) -> Result<bool, DomainError> {
        Ok(hash == format!("hashed:{plain}"))
    }
}

pub struct FakeMetricsSnapshots(Mutex<Vec<MetricsSnapshot>>);

impl FakeMetricsSnapshots {
    pub fn empty() -> Self {
        Self(Mutex::new(vec![]))
    }

    pub fn snapshot(&self) -> Vec<MetricsSnapshot> {
        self.0.lock().unwrap().clone()
    }
}

impl Default for FakeMetricsSnapshots {
    fn default() -> Self {
        Self::empty()
    }
}

#[async_trait]
impl MetricsSnapshotRepositoryPort for FakeMetricsSnapshots {
    async fn save(&self, snapshot: &MetricsSnapshot) -> Result<(), DomainError> {
        self.0.lock().unwrap().push(snapshot.clone());
        Ok(())
    }

    async fn list_since(&self, since: DateTime<Utc>) -> Result<Vec<MetricsSnapshot>, DomainError> {
        Ok(filter_in(&self.0, |s| s.recorded_at >= since))
    }
}

/// Maps each `disk_path` to a size. A path with no entry reports 0, like `GitBackend::directory_size` on an empty bare
/// repo.
pub struct FakeDirectorySize(HashMap<String, u64>);

impl FakeDirectorySize {
    pub fn new(sizes: HashMap<String, u64>) -> Self {
        Self(sizes)
    }
}

impl DirectorySizePort for FakeDirectorySize {
    fn directory_size(&self, disk_path: &str) -> std::io::Result<u64> {
        Ok(*self.0.get(disk_path).unwrap_or(&0))
    }
}

pub struct FakeHealthCheck(DatabaseHealth);

impl FakeHealthCheck {
    pub fn new(health: DatabaseHealth) -> Self {
        Self(health)
    }
}

#[async_trait]
impl HealthCheckPort for FakeHealthCheck {
    async fn check(&self) -> DatabaseHealth {
        self.0.clone()
    }
}

pub struct FakeStorageHealthCheck(StorageHealth);

impl FakeStorageHealthCheck {
    pub fn new(health: StorageHealth) -> Self {
        Self(health)
    }
}

#[async_trait]
impl StorageHealthCheckPort for FakeStorageHealthCheck {
    async fn check(&self) -> StorageHealth {
        self.0.clone()
    }
}

/// In-memory `MergeRequestEventPort`. `failing()` returns one that fails every method with `Infrastructure`, to check
/// that best-effort callers swallow the error.
#[derive(Default)]
pub struct FakeMergeRequestEvents {
    events: Mutex<Vec<MergeRequestEvent>>,
    head_shas: Mutex<HashMap<Uuid, String>>,
    fail: bool,
}

impl FakeMergeRequestEvents {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    pub fn snapshot(&self) -> Vec<MergeRequestEvent> {
        self.events.lock().unwrap().clone()
    }

    fn check(&self) -> Result<(), DomainError> {
        if self.fail {
            Err(DomainError::Infrastructure(
                "fake merge request event store is failing".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl MergeRequestEventPort for FakeMergeRequestEvents {
    async fn record(&self, event: NewMergeRequestEvent) -> Result<MergeRequestEvent, DomainError> {
        self.check()?;
        let recorded = MergeRequestEvent {
            id: Uuid::new_v4(),
            merge_request_id: event.merge_request_id,
            actor_id: event.actor_id,
            kind: event.kind,
            payload: event.payload,
            created_at: Utc::now(),
        };
        self.events.lock().unwrap().push(recorded.clone());
        Ok(recorded)
    }

    async fn list(&self, merge_request_id: Uuid) -> Result<Vec<MergeRequestEvent>, DomainError> {
        self.check()?;
        let mut events: Vec<MergeRequestEvent> =
            filter_in(&self.events, |e| e.merge_request_id == merge_request_id);
        events.sort_by_key(|e| (e.created_at, e.id));
        Ok(events)
    }

    async fn head_sha(&self, merge_request_id: Uuid) -> Result<Option<String>, DomainError> {
        self.check()?;
        Ok(self
            .head_shas
            .lock()
            .unwrap()
            .get(&merge_request_id)
            .cloned())
    }

    async fn set_head_sha(&self, merge_request_id: Uuid, sha: &str) -> Result<(), DomainError> {
        self.check()?;
        self.head_shas
            .lock()
            .unwrap()
            .insert(merge_request_id, sha.to_string());
        Ok(())
    }
}

/// In-memory `TotpCredentialPort`. `set_last_used_step` is a real compare-and-swap under the mutex, like the SQL
/// `UPDATE ... WHERE last_used_step IS NULL OR last_used_step < $step`.
#[derive(Default)]
pub struct FakeTotp {
    credentials: Mutex<HashMap<Uuid, TotpCredential>>,
}

impl FakeTotp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn credential_of(&self, user_id: Uuid) -> Option<TotpCredential> {
        self.credentials.lock().unwrap().get(&user_id).cloned()
    }
}

#[async_trait]
impl TotpCredentialPort for FakeTotp {
    async fn get(&self, user_id: Uuid) -> Result<Option<TotpCredential>, DomainError> {
        Ok(self.credential_of(user_id))
    }

    /// Like `ON CONFLICT ... DO UPDATE ... WHERE confirmed = false`: never overwrites a confirmed row.
    async fn upsert(&self, credential: &TotpCredential) -> Result<bool, DomainError> {
        let mut credentials = self.credentials.lock().unwrap();
        if credentials
            .get(&credential.user_id)
            .is_some_and(|existing| existing.confirmed)
        {
            return Ok(false);
        }
        credentials.insert(credential.user_id, credential.clone());
        Ok(true)
    }

    async fn set_last_used_step(&self, user_id: Uuid, step: i64) -> Result<bool, DomainError> {
        let mut credentials = self.credentials.lock().unwrap();
        let Some(credential) = credentials.get_mut(&user_id) else {
            return Ok(false);
        };
        if credential.last_used_step.is_some_and(|last| last >= step) {
            return Ok(false);
        }
        credential.last_used_step = Some(step);
        Ok(true)
    }

    /// Like the SQL `UPDATE ... WHERE confirmed = false AND last_used_step = $expected_step`.
    async fn confirm(&self, user_id: Uuid, expected_step: i64) -> Result<bool, DomainError> {
        let mut credentials = self.credentials.lock().unwrap();
        match credentials.get_mut(&user_id) {
            Some(credential)
                if !credential.confirmed && credential.last_used_step == Some(expected_step) =>
            {
                credential.confirmed = true;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn delete(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.credentials.lock().unwrap().remove(&user_id);
        Ok(())
    }

    async fn confirmed_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        let credentials = self.credentials.lock().unwrap();
        Ok(user_ids
            .iter()
            .copied()
            .filter(|id| credentials.get(id).is_some_and(|c| c.confirmed))
            .collect())
    }
}

/// In-memory `WebauthnCredentialPort`, same rules as the real store: `credential_id` is unique across users and a
/// duplicate `insert` leaves the existing row alone, `delete` is scoped to the owner, `update_after_authentication`
/// stamps `last_used_at`, and `list_for_user` orders by `created_at` then `id`.
#[derive(Default)]
pub struct FakePasskeys {
    rows: Mutex<Vec<StoredPasskey>>,
}

impl FakePasskeys {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn of(&self, user_id: Uuid) -> Vec<StoredPasskey> {
        let mut rows: Vec<StoredPasskey> = filter_in(&self.rows, |row| row.user_id == user_id);
        rows.sort_by_key(|row| (row.created_at, row.id));
        rows
    }

    /// A stored passkey with no real authenticator behind it, for tests that just need "the user has one".
    pub fn seed(&self, user_id: Uuid, name: &str) -> StoredPasskey {
        let passkey = StoredPasskey {
            id: Uuid::new_v4(),
            user_id,
            name: name.to_string(),
            credential_id: Uuid::new_v4().as_bytes().to_vec(),
            passkey_json: "{}".to_string(),
            created_at: Utc::now(),
            last_used_at: None,
        };
        self.rows.lock().unwrap().push(passkey.clone());
        passkey
    }
}

#[async_trait]
impl WebauthnCredentialPort for FakePasskeys {
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError> {
        Ok(self.of(user_id))
    }

    async fn count_for_user(&self, user_id: Uuid) -> Result<i64, DomainError> {
        Ok(self.of(user_id).len() as i64)
    }

    async fn insert(&self, passkey: &StoredPasskey) -> Result<bool, DomainError> {
        let mut rows = self.rows.lock().unwrap();
        if rows
            .iter()
            .any(|row| row.credential_id == passkey.credential_id)
        {
            return Ok(false);
        }
        rows.push(passkey.clone());
        Ok(true)
    }

    async fn update_after_authentication(
        &self,
        id: Uuid,
        passkey_json: &str,
    ) -> Result<(), DomainError> {
        update_in(
            &self.rows,
            |row| row.id == id,
            |row| {
                row.passkey_json = passkey_json.to_string();
                row.last_used_at = Some(Utc::now());
            },
        );
        Ok(())
    }

    async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<bool, DomainError> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|row| !(row.id == id && row.user_id == user_id));
        Ok(rows.len() < before)
    }

    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.rows
            .lock()
            .unwrap()
            .retain(|row| row.user_id != user_id);
        Ok(())
    }

    async fn user_ids_with_passkeys(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        let rows = self.rows.lock().unwrap();
        Ok(user_ids
            .iter()
            .copied()
            .filter(|id| rows.iter().any(|row| row.user_id == *id))
            .collect())
    }
}

/// One expiring token row per user, shared by both token fakes below: `(user, token hash, expiry)`.
#[derive(Default)]
struct TokenRows(Mutex<Vec<(Uuid, String, DateTime<Utc>)>>);

impl TokenRows {
    /// Replaces the user's row whatever its expiry.
    fn insert(&self, user_id: Uuid, token_hash: &str, expires_at: DateTime<Utc>) {
        let mut rows = self.0.lock().unwrap();
        rows.retain(|(id, _, _)| *id != user_id);
        rows.push((user_id, token_hash.to_string(), expires_at));
    }

    fn remove(&self, user_id: Uuid) {
        self.0.lock().unwrap().retain(|(id, _, _)| *id != user_id);
    }

    fn snapshot(&self) -> Vec<(Uuid, String, DateTime<Utc>)> {
        self.0.lock().unwrap().clone()
    }

    fn row_of(&self, user_id: Uuid) -> Option<(String, DateTime<Utc>)> {
        find_in(&self.0, |(id, _, _)| *id == user_id)
            .map(|(_, hash, expires_at)| (hash, expires_at))
    }

    /// Deletes and returns the row of an unexpired token. An expired row stays and is never consumed.
    fn consume(&self, token_hash: &str) -> Option<(Uuid, DateTime<Utc>)> {
        let mut rows = self.0.lock().unwrap();
        let position = rows
            .iter()
            .position(|(_, hash, expires_at)| hash == token_hash && *expires_at > Utc::now())?;
        let (user_id, _, expires_at) = rows.remove(position);
        Some((user_id, expires_at))
    }
}

/// In-memory `UserInvitationPort`, like the real store: one row per user, and `consume` deletes atomically and only if
/// not expired.
#[derive(Default)]
pub struct FakeInvitations {
    rows: TokenRows,
}

impl FakeInvitations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a row as is, for example already expired.
    pub fn insert(&self, user_id: Uuid, token_hash: &str, expires_at: DateTime<Utc>) {
        self.rows.insert(user_id, token_hash, expires_at);
    }

    /// Drops the user's row, as an activation would, to simulate that race.
    pub fn remove(&self, user_id: Uuid) {
        self.rows.remove(user_id);
    }

    pub fn snapshot(&self) -> Vec<(Uuid, String, DateTime<Utc>)> {
        self.rows.snapshot()
    }

    pub fn row_of(&self, user_id: Uuid) -> Option<(String, DateTime<Utc>)> {
        self.rows.row_of(user_id)
    }
}

#[async_trait]
impl UserInvitationPort for FakeInvitations {
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        self.insert(user_id, token_hash, expires_at);
        Ok(())
    }

    async fn renew(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        let mut rows = self.rows.0.lock().unwrap();
        let Some(row) = rows.iter_mut().find(|(id, _, _)| *id == user_id) else {
            return Ok(false);
        };
        *row = (user_id, token_hash.to_string(), expires_at);
        Ok(true)
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<Invitation>, DomainError> {
        Ok(self
            .rows
            .consume(token_hash)
            .map(|(user_id, expires_at)| Invitation {
                user_id,
                expires_at,
            }))
    }

    async fn expiries(&self, user_ids: &[Uuid]) -> Result<Vec<(Uuid, DateTime<Utc>)>, DomainError> {
        Ok(filter_in(&self.rows.0, |(id, _, _)| user_ids.contains(id))
            .into_iter()
            .map(|(id, _, expires_at)| (id, expires_at))
            .collect())
    }
}

/// In-memory `PasswordResetPort`, like `FakeInvitations`: one row per user, and `consume` deletes atomically and only if
/// not expired.
#[derive(Default)]
pub struct FakePasswordResets {
    rows: TokenRows,
}

impl FakePasswordResets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a row as is, for example already expired.
    pub fn insert(&self, user_id: Uuid, token_hash: &str, expires_at: DateTime<Utc>) {
        self.rows.insert(user_id, token_hash, expires_at);
    }

    pub fn snapshot(&self) -> Vec<(Uuid, String, DateTime<Utc>)> {
        self.rows.snapshot()
    }

    pub fn row_of(&self, user_id: Uuid) -> Option<(String, DateTime<Utc>)> {
        self.rows.row_of(user_id)
    }
}

#[async_trait]
impl PasswordResetPort for FakePasswordResets {
    async fn replace(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        self.insert(user_id, token_hash, expires_at);
        Ok(())
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<PasswordReset>, DomainError> {
        Ok(self
            .rows
            .consume(token_hash)
            .map(|(user_id, expires_at)| PasswordReset {
                user_id,
                expires_at,
            }))
    }

    /// Any row counts, expired or not, like the SQL `EXISTS`.
    async fn is_pending(&self, user_id: Uuid) -> Result<bool, DomainError> {
        Ok(self.row_of(user_id).is_some())
    }

    /// Like `INSERT ... ON CONFLICT DO NOTHING`: never overwrites a row the user already has.
    async fn restore(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        let mut rows = self.rows.0.lock().unwrap();
        if rows
            .iter()
            .any(|(id, hash, _)| *id == user_id || hash == token_hash)
        {
            return Ok(false);
        }
        rows.push((user_id, token_hash.to_string(), expires_at));
        Ok(true)
    }
}

pub struct FakeRegistration(Mutex<bool>);

impl FakeRegistration {
    pub fn new(enabled: bool) -> Self {
        Self(Mutex::new(enabled))
    }
}

#[async_trait]
impl RegistrationSettingsPort for FakeRegistration {
    async fn is_enabled(&self) -> Result<bool, DomainError> {
        Ok(*self.0.lock().unwrap())
    }

    async fn set_enabled(&self, enabled: bool) -> Result<(), DomainError> {
        *self.0.lock().unwrap() = enabled;
        Ok(())
    }
}

/// Mail settings that are either there or never were configured.
pub struct FakeSmtpSettings(Mutex<Option<SmtpSettings>>);

impl FakeSmtpSettings {
    pub fn configured() -> Self {
        Self(Mutex::new(Some(SmtpSettings {
            host: "smtp.example.com".to_string(),
            port: 587,
            security: SmtpSecurity::StartTls,
            username: String::new(),
            password: None,
            from_address: "noreply@example.com".to_string(),
            from_name: "FerrisGit".to_string(),
        })))
    }

    pub fn unconfigured() -> Self {
        Self(Mutex::new(None))
    }
}

#[async_trait]
impl SmtpSettingsPort for FakeSmtpSettings {
    async fn get(&self) -> Result<Option<SmtpSettings>, DomainError> {
        Ok(self.0.lock().unwrap().clone())
    }

    async fn save(&self, settings: &SmtpSettings) -> Result<(), DomainError> {
        *self.0.lock().unwrap() = Some(settings.clone());
        Ok(())
    }
}

/// A mail port that records what it was asked to send, and can be told to refuse everything.
#[derive(Default)]
pub struct FakeEmail {
    sent: Mutex<Vec<(String, String, String)>>,
    fail: std::sync::atomic::AtomicBool,
}

impl FakeEmail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refuse_everything(&self) {
        self.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn recover(&self) {
        self.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// `(to, subject, text body)` of everything delivered.
    pub fn sent(&self) -> Vec<(String, String, String)> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl EmailPort for FakeEmail {
    async fn send(
        &self,
        to: &str,
        subject: &str,
        text_body: &str,
        _html_body: &str,
    ) -> Result<(), DomainError> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(DomainError::Infrastructure("smtp down".to_string()));
        }
        self.sent.lock().unwrap().push((
            to.to_string(),
            subject.to_string(),
            text_body.to_string(),
        ));
        Ok(())
    }
}

/// The accounts waiting for their activation. Like the real store it joins the accounts to their invitations: one
/// without an invitation row is active and is not listed, and the link expiry is the invitation's.
pub struct FakePendingAccounts {
    accounts: Mutex<Vec<PendingAccount>>,
    invitations: Arc<FakeInvitations>,
    deleted: Mutex<Vec<Uuid>>,
    refuse_delete_of: Mutex<Vec<Uuid>>,
    activated_before_delete: Mutex<Vec<Uuid>>,
}

impl FakePendingAccounts {
    pub fn new(accounts: Vec<PendingAccount>, invitations: Arc<FakeInvitations>) -> Self {
        Self {
            accounts: Mutex::new(accounts),
            invitations,
            deleted: Mutex::new(Vec::new()),
            refuse_delete_of: Mutex::new(Vec::new()),
            activated_before_delete: Mutex::new(Vec::new()),
        }
    }

    /// Deleting this account fails, as a database error would.
    pub fn refuse_to_delete(&self, user_id: Uuid) {
        self.refuse_delete_of.lock().unwrap().push(user_id);
    }

    /// The account gets activated between `list` and `delete`.
    pub fn activate_before_delete(&self, user_id: Uuid) {
        self.activated_before_delete.lock().unwrap().push(user_id);
    }

    pub fn deleted(&self) -> Vec<Uuid> {
        self.deleted.lock().unwrap().clone()
    }
}

#[async_trait]
impl PendingAccountPort for FakePendingAccounts {
    async fn list(&self) -> Result<Vec<PendingAccount>, DomainError> {
        let mut pending: Vec<PendingAccount> = self
            .accounts
            .lock()
            .unwrap()
            .iter()
            .filter_map(|account| {
                let (_, link_expires_at) = self.invitations.row_of(account.user_id)?;
                Some(PendingAccount {
                    link_expires_at,
                    ..account.clone()
                })
            })
            .collect();
        pending.sort_by_key(|account| account.created_at);
        Ok(pending)
    }

    async fn delete(&self, user_id: Uuid) -> Result<bool, DomainError> {
        if self.refuse_delete_of.lock().unwrap().contains(&user_id) {
            return Err(DomainError::Infrastructure("cannot delete".to_string()));
        }
        if self
            .activated_before_delete
            .lock()
            .unwrap()
            .contains(&user_id)
        {
            return Ok(false);
        }
        self.accounts
            .lock()
            .unwrap()
            .retain(|a| a.user_id != user_id);
        self.invitations.remove(user_id);
        self.deleted.lock().unwrap().push(user_id);
        Ok(true)
    }
}

pub struct FakePublicPagesSettings(Mutex<PublicPagesSettings>);

impl FakePublicPagesSettings {
    pub fn new(settings: PublicPagesSettings) -> Self {
        Self(Mutex::new(settings))
    }
}

#[async_trait]
impl PublicPagesSettingsPort for FakePublicPagesSettings {
    async fn get(&self) -> Result<PublicPagesSettings, DomainError> {
        Ok(*self.0.lock().unwrap())
    }

    async fn update(
        &self,
        update: PublicPagesSettingsUpdate,
    ) -> Result<PublicPagesSettings, DomainError> {
        let mut settings = self.0.lock().unwrap();
        if let Some(enabled) = update.public_pages_enabled {
            settings.public_pages_enabled = enabled;
        }
        if let Some(enabled) = update.seo_indexing_enabled {
            settings.seo_indexing_enabled = enabled;
        }
        Ok(*settings)
    }
}

/// In-memory `BackupCodePort` over salted hashes like the real adapter: `try_consume` checks the plaintext against
/// every unused hash of the user and marks the match used, atomically.
#[derive(Default)]
pub struct FakeBackupCodes {
    by_user: Mutex<HashMap<Uuid, Vec<(String, bool)>>>,
}

impl FakeBackupCodes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hashes_of(&self, user_id: Uuid) -> Vec<String> {
        self.by_user
            .lock()
            .unwrap()
            .get(&user_id)
            .map(|codes| codes.iter().map(|(hash, _)| hash.clone()).collect())
            .unwrap_or_default()
    }
}

#[async_trait]
impl BackupCodePort for FakeBackupCodes {
    async fn replace_all(&self, user_id: Uuid, code_hashes: &[String]) -> Result<(), DomainError> {
        self.by_user.lock().unwrap().insert(
            user_id,
            code_hashes
                .iter()
                .map(|hash| (hash.clone(), false))
                .collect(),
        );
        Ok(())
    }

    async fn try_consume(&self, user_id: Uuid, plaintext_code: &str) -> Result<bool, DomainError> {
        let mut by_user = self.by_user.lock().unwrap();
        let Some(codes) = by_user.get_mut(&user_id) else {
            return Ok(false);
        };
        let Some(entry) = codes.iter_mut().find(|(hash, used)| {
            !*used && crate::mfa_crypto::verify_backup_code(plaintext_code, hash)
        }) else {
            return Ok(false);
        };
        entry.1 = true;
        Ok(true)
    }

    async fn count_unused(&self, user_id: Uuid) -> Result<i64, DomainError> {
        Ok(self
            .by_user
            .lock()
            .unwrap()
            .get(&user_id)
            .map_or(0, |codes| codes.iter().filter(|(_, used)| !*used).count()) as i64)
    }

    async fn delete_all(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.by_user.lock().unwrap().remove(&user_id);
        Ok(())
    }
}
