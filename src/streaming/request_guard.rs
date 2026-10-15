//! A canceled agent must also cancel its nested provider request.
pub(crate) struct AbortOnDrop(tokio::task::AbortHandle);

impl AbortOnDrop {
    pub(crate) fn new<T>(task: &tokio::task::JoinHandle<T>) -> Self {
        Self(task.abort_handle())
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dropping_parent_cancels_its_inflight_child() {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (closed_tx, closed_rx) = tokio::sync::oneshot::channel::<()>();
        let parent = tokio::spawn(async move {
            let child = tokio::spawn(async move {
                let _closed = closed_tx;
                let _ = ready_tx.send(());
                std::future::pending::<()>().await;
            });
            let _cancel = AbortOnDrop::new(&child);
            let _ = child.await;
        });
        ready_rx.await.unwrap();
        parent.abort();
        let _ = parent.await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), closed_rx)
                .await
                .unwrap()
                .is_err()
        );
    }
}
