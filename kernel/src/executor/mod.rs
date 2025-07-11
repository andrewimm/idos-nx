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
use spin::RwLock;

pub struct Executor<E: Ord + Copy + Sized + Unpin> {
    tasks: BTreeMap<AsyncTaskId, AsyncTask>,
    next_task_id: AsyncTaskId,
    run_queue: Arc<RwLock<VecDeque<AsyncTaskId>>>,
    wakers: WakerRegistry<E>,
}

impl<E: Ord + Copy + Sized + Unpin> Executor<E> {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            next_task_id: 1,
            run_queue: Arc::new(RwLock::new(VecDeque::new())),
            wakers: WakerRegistry::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn waker_registry(&self) -> WakerRegistry<E> {
        self.wakers.clone()
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
        self.run_queue.write().push_back(task_id);
    }

    pub fn poll_tasks(&mut self) {
        let run_queue = core::mem::take(&mut *self.run_queue.write());
        for task_id in run_queue {
            if let Some(task) = self.tasks.get_mut(&task_id) {
                let waker = task.waker.take().unwrap_or_else(|| {
                    Waker::from(Arc::new(TaskWaker {
                        task_id,
                        run_queue: self.run_queue.clone(),
                    }))
                });

                let mut context = Context::from_waker(&waker);

                match task.future.as_mut().poll(&mut context) {
                    Poll::Ready(()) => {
                        self.tasks.remove(&task_id);
                    }
                    Poll::Pending => {
                        task.waker = Some(waker);
                        self.run_queue.write().push_back(task_id);
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
    run_queue: Arc<RwLock<VecDeque<AsyncTaskId>>>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.run_queue.write().push_back(self.task_id);
    }
}

struct WaitEventState {
    fired: bool,
    refcount: usize,
}

impl WaitEventState {
    pub fn new() -> Self {
        Self {
            fired: false,
            refcount: 1,
        }
    }
}

#[derive(Clone)]
pub struct WakerRegistry<E: Ord + Copy + Sized + Unpin> {
    wakers: Arc<RwLock<BTreeMap<E, Vec<Waker>>>>,
    events: Arc<RwLock<BTreeMap<E, WaitEventState>>>,
}

impl<E: Ord + Copy + Sized + Unpin> WakerRegistry<E> {
    pub fn new() -> Self {
        Self {
            wakers: Arc::new(RwLock::new(BTreeMap::new())),
            events: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn register(&self, event: E, waker: Waker) {
        self.wakers
            .write()
            .entry(event)
            .or_insert_with(Vec::new)
            .push(waker);

        self.events
            .write()
            .entry(event)
            .and_modify(|state| state.refcount += 1)
            .or_insert_with(WaitEventState::new);
    }

    pub fn notify_event(&self, event: &E) {
        if let Some(wakers) = self.wakers.write().remove(event) {
            for waker in wakers {
                waker.wake();
            }
        }

        if let Some(event_state) = self.events.write().get_mut(event) {
            event_state.fired = true;
        }
    }

    pub fn check_event(&self, event: &E) -> bool {
        if let Some(event_state) = self.events.write().get_mut(event) {
            if event_state.fired {
                event_state.refcount -= 1;
            }
            return event_state.fired;
        }
        false
    }
}

/// WaitForEvent is a Future that waits on a generic event condition.
/// Individual instances of the Executor can use their own event implementation.
/// A network executor might wait on an enum whose values represent different
/// network resolution states, while a floppy disk executor might wait on
/// controller changes.
pub struct WaitForEvent<E: Ord + Copy + Sized + Unpin> {
    event: E,
    waker_registry: WakerRegistry<E>,
    registered: bool,
}

impl<E: Ord + Copy + Sized + Unpin> WaitForEvent<E> {
    pub fn new(event: E, waker_registry: WakerRegistry<E>) -> Self {
        Self {
            event,
            waker_registry,
            registered: false,
        }
    }
}

impl<E: Ord + Copy + Sized + Unpin> Future for WaitForEvent<E> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.registered {
            self.waker_registry
                .register(self.event.clone(), cx.waker().clone());
            self.registered = true;
        } else {
            if self.waker_registry.check_event(&self.event) {
                return Poll::Ready(());
            }
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll},
    };

    use alloc::sync::Arc;

    use super::{Executor, WakerRegistry};

    #[test_case]
    fn immediately_ready_future() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut executor = Executor::<u32>::new();

        for _ in 0..10 {
            let counter_clone = counter.clone();
            executor.spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 10);
        assert!(executor.is_empty());
    }

    #[test_case]
    fn pending_future() {
        #[derive(Default)]
        struct PendingOnce {
            should_resolve: bool,
        }

        impl Future for PendingOnce {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
                if self.should_resolve {
                    Poll::Ready(())
                } else {
                    self.should_resolve = true;
                    Poll::Pending
                }
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let mut executor = Executor::<u32>::new();

        for _ in 0..10 {
            let counter_clone = counter.clone();
            executor.spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                PendingOnce::default().await;
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 10);

        // Simulate waking up tasks
        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 20);
        assert!(executor.is_empty());
    }

    #[test_case]
    fn waiting_on_event() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut executor = Executor::<u32>::new();

        async fn async_task(
            event: usize,
            counter: Arc<AtomicUsize>,
            waker_registry: WakerRegistry<u32>,
        ) {
            counter.fetch_add(1, Ordering::SeqCst);
            super::WaitForEvent::new(event as u32, waker_registry).await;
            counter.fetch_add(1, Ordering::SeqCst);
        }

        for i in 0..10 {
            let counter_clone = counter.clone();
            let waker_registry_clone = executor.waker_registry();
            executor.spawn(async_task(i, counter_clone, waker_registry_clone));
        }

        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 10);

        // Nothing should wake yet
        executor.poll_tasks();
        // Simulate notifying events
        for i in 0..10 {
            executor.waker_registry().notify_event(&i);
            executor.poll_tasks();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 20);
        assert!(executor.is_empty());
    }

    #[test_case]
    fn wake_multiple_on_same_event() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut executor = Executor::<u32>::new();

        async fn async_task(counter: Arc<AtomicUsize>, waker_registry: WakerRegistry<u32>) {
            counter.fetch_add(1, Ordering::SeqCst);
            super::WaitForEvent::new(10, waker_registry).await;
            counter.fetch_add(1, Ordering::SeqCst);
        }

        for i in 0..10 {
            let counter_clone = counter.clone();
            let waker_registry_clone = executor.waker_registry();
            executor.spawn(async_task(counter_clone, waker_registry_clone));
        }

        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 10);

        // Nothing should wake yet
        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 10);
        // Simulate notifying events
        for i in 0..9 {
            executor.waker_registry().notify_event(&i);
        }
        executor.poll_tasks();
        assert_eq!(counter.load(Ordering::SeqCst), 10);
        executor.waker_registry().notify_event(&10);
        executor.poll_tasks();

        assert_eq!(counter.load(Ordering::SeqCst), 20);
        assert!(executor.is_empty());
    }
}
