use axum::body::Bytes;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const TOTAL_STORED_LIMIT: usize = 32 * 1024 * 1024;

pub struct BodyBudget {
    permits: std::sync::Arc<Semaphore>,
}

impl BodyBudget {
    pub const PER_REQUEST_LIMIT: usize = 8 * 1024 * 1024;

    pub fn new() -> Self {
        Self {
            permits: std::sync::Arc::new(Semaphore::new(TOTAL_STORED_LIMIT)),
        }
    }

    pub fn stored_bytes(&self) -> usize {
        TOTAL_STORED_LIMIT - self.permits.available_permits()
    }

    pub async fn read(&self, mut incoming: Incoming) -> Result<BudgetedBody, BodyReadError> {
        let mut bytes = Vec::new();
        let mut permit: Option<OwnedSemaphorePermit> = None;
        while let Some(frame) = incoming.frame().await {
            let frame = frame.map_err(|_| BodyReadError::Read)?;
            let Ok(data) = frame.into_data() else {
                continue;
            };
            let next_length = bytes
                .len()
                .checked_add(data.len())
                .ok_or(BodyReadError::TooLarge)?;
            if next_length > Self::PER_REQUEST_LIMIT {
                return Err(BodyReadError::TooLarge);
            }
            let count = u32::try_from(data.len()).map_err(|_| BodyReadError::TooLarge)?;
            if count > 0 {
                let new_permit = self
                    .permits
                    .clone()
                    .try_acquire_many_owned(count)
                    .map_err(|_| BodyReadError::MemoryFull)?;
                if let Some(existing) = &mut permit {
                    existing.merge(new_permit);
                } else {
                    permit = Some(new_permit);
                }
            }
            bytes.extend_from_slice(&data);
        }
        Ok(BudgetedBody {
            bytes: guarded_bytes(bytes, permit),
        })
    }
}

#[derive(Clone)]
pub struct BudgetedBody {
    bytes: Bytes,
}

impl BudgetedBody {
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

struct GuardedBytes {
    bytes: Vec<u8>,
    _permit: Option<OwnedSemaphorePermit>,
}

impl AsRef<[u8]> for GuardedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

fn guarded_bytes(bytes: Vec<u8>, permit: Option<OwnedSemaphorePermit>) -> Bytes {
    Bytes::from_owner(GuardedBytes {
        bytes,
        _permit: permit,
    })
}

pub enum BodyReadError {
    TooLarge,
    MemoryFull,
    Read,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Semaphore;

    use super::guarded_bytes;

    #[test]
    fn permit_lives_until_the_last_bytes_clone() {
        let semaphore = Arc::new(Semaphore::new(3));
        let permit = semaphore
            .clone()
            .try_acquire_many_owned(3)
            .expect("test permit");
        let bytes = guarded_bytes(vec![1, 2, 3], Some(permit));
        let clone = bytes.clone();

        drop(bytes);
        assert_eq!(semaphore.available_permits(), 0);
        drop(clone);
        assert_eq!(semaphore.available_permits(), 3);
    }
}
