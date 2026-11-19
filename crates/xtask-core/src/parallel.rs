//! Role: 入力ごとに一つの scoped thread で処理し、panic を入力ごとの誤りにする。
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::thread;

/// 入力ごとの結果（入力と同じ順）。f が panic した入力は Err（panic の文）。
/// 同時に動く thread は CPU の数まで。
pub(crate) fn map<T: Sync, R: Send>(
    inputs: &[T],
    f: impl Fn(&T) -> R + Sync,
) -> Vec<Result<R, String>> {
    let limit = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    map_with_limit(inputs, limit, f)
}

pub(crate) fn map_with_limit<T: Sync, R: Send>(
    inputs: &[T],
    limit: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<Result<R, String>> {
    let limit = limit.max(1);
    let mut finished = BTreeMap::new();
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel::<usize>();
        let mut running = BTreeMap::new();
        let mut next = 0;
        loop {
            while running.len() < limit && next < inputs.len() {
                let signal = Finished {
                    index: next,
                    sender: sender.clone(),
                };
                let (input, f) = (&inputs[next], &f);
                running.insert(
                    next,
                    scope.spawn(move || {
                        let _signal = signal;
                        f(input)
                    }),
                );
                next += 1;
            }
            if running.is_empty() {
                break;
            }
            let index = receiver.recv().expect("a running worker holds a sender");
            let handle = running
                .remove(&index)
                .expect("the finished worker is running");
            finished.insert(
                index,
                handle.join().map_err(|payload| panic_message(&*payload)),
            );
        }
    });
    finished.into_values().collect()
}

/// worker の終わり（正常の return と panic の巻戻しの両方）で index を送る。
struct Finished {
    index: usize,
    sender: mpsc::Sender<usize>,
}

impl Drop for Finished {
    fn drop(&mut self) {
        let _ = self.sender.send(self.index);
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "worker panicked".to_owned()
    }
}

#[cfg(test)]
mod tests {
    //! Role: 並列処理の結果の順序、panic の隔離、送るだけの型の扱いを検証する。
    use super::*;

    #[test]
    fn results_follow_input_order() {
        assert_eq!(
            map_with_limit(&[3, 1, 2], 2, |x| x * 10),
            vec![Ok(30), Ok(10), Ok(20)]
        );
    }

    #[test]
    fn panic_isolated_with_one_worker() {
        let results = map_with_limit(&[0, 1], 1, |&x| if x == 0 { panic!("boom") } else { x });
        assert_eq!(results, vec![Err("boom".to_owned()), Ok(1)]);
    }

    #[test]
    fn every_input_may_panic() {
        let results: Vec<Result<u8, String>> =
            map_with_limit(&[0, 1, 2], 2, |&x: &u8| -> u8 { panic!("p{x}") });
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(Result::is_err));
    }

    #[test]
    fn send_only_results() {
        let results = map_with_limit(&[1_u8, 2, 3], 2, |&x| std::cell::Cell::new(x));
        let values: Vec<u8> = results
            .into_iter()
            .map(|result| result.expect("no panic").get())
            .collect();
        assert_eq!(values, vec![1, 2, 3]);
    }
}
