use core::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    task::Wake,
    vec::Vec,
};

pub struct Executor {
    tasks: BTreeMap<AsyncTaskId, AsyncTask>,
    next_task_id: AsyncTaskId,
    run_queue: VecDeque<AsyncTaskId>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            next_task_id: 1,
            run_queue: VecDeque::new(),
        }
    }

    pub fn spawn<F>(&mut self, future: F) -> ()
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task_id = self.next_task_id;
        self.next_task_id += 1;

        let task = AsyncTask {
            future: Box::pin(future),
            waker: None,
        };
        self.tasks.insert(task_id, task);
        self.run_queue.push_back(task_id);
    }

    pub fn poll_tasks(&mut self) {
        let run_queue = core::mem::take(&mut self.run_queue);
        for task_id in run_queue {
            if let Some(task) = self.tasks.get_mut(&task_id) {
                let waker = task
                    .waker
                    .take()
                    .unwrap_or_else(|| Waker::from(Arc::new(TaskWaker { task_id })));

                let mut context = Context::from_waker(&waker);

                match task.future.as_mut().poll(&mut context) {
                    Poll::Ready(()) => {
                        self.tasks.remove(&task_id);
                    }
                    Poll::Pending => {
                        task.waker = Some(waker);
                        self.run_queue.push_back(task_id);
                    }
                }
            }
        }
    }
}

struct AsyncTask {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    waker: Option<Waker>,
}

type AsyncTaskId = u32;

struct TaskWaker {
    task_id: AsyncTaskId,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        // TODO: wake
    }
}

#[derive(Clone)]
struct WakerRegistry<E: Ord> {
    wakers: Arc<RefCell<BTreeMap<E, Vec<Waker>>>>,
}

impl<E: Ord> WakerRegistry<E> {
    pub fn new() -> Self {
        Self {
            wakers: Arc::new(RefCell::new(BTreeMap::new())),
        }
    }

    pub fn register(&self, event: E, waker: Waker) {
        self.wakers
            .borrow_mut()
            .entry(event)
            .or_insert_with(Vec::new)
            .push(waker);
    }

    pub fn notify_event(&self, event: &E) {
        if let Some(wakers) = self.wakers.borrow_mut().remove(event) {
            for waker in wakers {
                waker.wake();
            }
        }
    }
}

/// WaitForEvent is a Future that waits on a generic event condition.
/// Individual instances of the Executor can use their own event implementation.
/// A network executor might wait on an enum whose values represent different
/// network resolution states, while a floppy disk executor might wait on
/// controller changes.
pub struct WaitForEvent<E: Ord> {
    event: E,
    waker_registry: WakerRegistry<E>,
    registered: bool,
}

impl<E: Ord> WaitForEvent<E> {
    pub fn new(event: E, waker_registry: WakerRegistry<E>) -> Self {
        Self {
            event,
            waker_registry,
            registered: false,
        }
    }
}

impl<E: Ord> Future for WaitForEvent<E> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(())
    }
}

#[cfg(test)]
mod tests {}
