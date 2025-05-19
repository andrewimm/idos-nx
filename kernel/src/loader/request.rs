use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::{collections::VecDeque, string::String};
use spin::RwLock;

use crate::{
    memory::address::VirtualAddress,
    sync::futex::{futex_wait, futex_wake},
    task::id::TaskID,
};

pub struct LoaderRequest {
    pub task: TaskID,
    pub path: String,
}

pub struct RequestQueue {
    requests: RwLock<VecDeque<LoaderRequest>>,
    request_count: AtomicUsize,
}

impl RequestQueue {
    pub const fn new() -> Self {
        Self {
            requests: RwLock::new(VecDeque::new()),
            request_count: AtomicUsize::new(0),
        }
    }

    pub fn get_request_count_address(&self) -> VirtualAddress {
        VirtualAddress::new(self.request_count.as_ptr() as u32)
    }

    pub fn add_request(&self, task: TaskID, path: &str) {
        let req = LoaderRequest {
            task,
            path: String::from(path),
        };

        self.requests.write().push_back(req);

        self.request_count.fetch_add(1, Ordering::SeqCst);
        futex_wake(self.get_request_count_address(), 1);
    }

    pub fn wait_on_request(&self) -> LoaderRequest {
        loop {
            if self.request_count.load(Ordering::SeqCst) == 0 {
                futex_wait(self.get_request_count_address(), 0, None);
            }
            if self.request_count.swap(0, Ordering::SeqCst) != 0 {
                let front = self.requests.write().pop_front();
                if let Some(request) = front {
                    return request;
                }
            }
        }
    }
}
