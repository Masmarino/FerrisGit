//! Serialises a user's first MFA setup.
//!
//! The setup routes check "no factor yet", write the factor and backup codes, then spend the `mfa-pending` token.
//! Two concurrent setups would both pass the check and the loser would overwrite the codes the winner is about to
//! show. Holding this lock from the check to the token spend makes the second one see the first one's result.
//!
//! Striped instead of one lock per user, so there is no map to grow. In memory only, so it protects one replica,
//! like the ceremonies and the spent-token set.

use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

const STRIPES: usize = 64;

pub struct FirstSetupLocks {
    stripes: Vec<Mutex<()>>,
}

impl Default for FirstSetupLocks {
    fn default() -> Self {
        Self {
            stripes: (0..STRIPES).map(|_| Mutex::new(())).collect(),
        }
    }
}

impl FirstSetupLocks {
    pub async fn lock(&self, user_id: Uuid) -> MutexGuard<'_, ()> {
        let stripe = (user_id.as_u128() % STRIPES as u128) as usize;
        self.stripes[stripe].lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn a_second_setup_of_the_same_user_waits_for_the_first() {
        let locks = Arc::new(FirstSetupLocks::default());
        let user = Uuid::new_v4();
        let first = locks.lock(user).await;

        let waiting = {
            let locks = locks.clone();
            tokio::spawn(async move {
                let _guard = locks.lock(user).await;
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !waiting.is_finished(),
            "still waiting while the first setup runs"
        );

        drop(first);
        tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("released")
            .unwrap();
    }

    #[tokio::test]
    async fn the_lock_is_the_same_for_the_same_user_whatever_the_call() {
        let locks = FirstSetupLocks::default();
        let user = Uuid::new_v4();
        let _held = locks.lock(user).await;

        assert!(
            tokio::time::timeout(Duration::from_millis(50), locks.lock(user))
                .await
                .is_err()
        );
    }
}
