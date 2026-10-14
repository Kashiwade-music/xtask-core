//! Role: 独立した検査を上限付きで並列実行し、入力順に結果を返す。
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) fn map<T: Sync, R: Send>(
    inputs: &[T],
    operation: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>, String> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8)
        .min(inputs.len());
    map_with_workers(inputs, operation, workers)
}

fn map_with_workers<T: Sync, R: Send>(
    inputs: &[T],
    operation: impl Fn(&T) -> R + Sync,
    workers: usize,
) -> Result<Vec<R>, String> {
    if workers <= 1 {
        return Ok(inputs.iter().map(operation).collect());
    }
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let next = &next;
                let operation = &operation;
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(input) = inputs.get(index) else {
                            break;
                        };
                        results.push((index, operation(input)));
                    }
                    results
                })
            })
            .collect();
        let mut results = Vec::with_capacity(inputs.len());
        let mut failed = false;
        // Join every worker even if one panics, so scope does not rethrow it.
        for handle in handles {
            match handle.join() {
                Ok(completed) => results.extend(completed),
                Err(_) => failed = true,
            }
        }
        if failed {
            return Err("parallel check worker panicked".into());
        }
        results.sort_unstable_by_key(|(index, _)| *index);
        Ok(results.into_iter().map(|(_, result)| result).collect())
    })
}

#[cfg(test)]
mod tests {
    //! Role: 並列検査の順序、全件実行、およびworkerの失敗通知を検証する。
    use super::*;

    #[test]
    fn results_match_sequential_order_including_errors() {
        let inputs: Vec<_> = (0..100).collect();
        let operation = |&value: &usize| {
            if value % 7 == 0 {
                Err(value)
            } else {
                Ok(value * 2)
            }
        };
        let sequential = map_with_workers(&inputs, operation, 1).unwrap();
        assert_eq!(map_with_workers(&inputs, operation, 8).unwrap(), sequential);
        assert!(
            map_with_workers::<usize, usize>(&[], |value| *value, 8)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn worker_panic_is_reported_after_all_workers_are_joined() {
        let error = map_with_workers(
            &[0, 1, 2],
            |&value| {
                assert_ne!(value, 1, "worker failure");
                value
            },
            2,
        )
        .unwrap_err();
        assert!(error.contains("worker panicked"));
    }
}
