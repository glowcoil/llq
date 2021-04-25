#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cell::Cell;
use core::marker::PhantomData;
use core::mem;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, Ordering};

pub struct Node<T> {
    inner: NonNull<NodeInner<T>>,
    phantom: PhantomData<T>,
}

unsafe impl<T: Send> Send for Node<T> {}
unsafe impl<T: Sync> Sync for Node<T> {}

struct NodeInner<T> {
    next: AtomicPtr<NodeInner<T>>,
    data: MaybeUninit<T>,
}

impl<T> Node<T> {
    pub fn new(data: T) -> Node<T> {
        Node {
            inner: unsafe {
                NonNull::new_unchecked(Box::into_raw(Box::new(NodeInner {
                    next: AtomicPtr::new(ptr::null_mut()),
                    data: MaybeUninit::new(data),
                })))
            },
            phantom: PhantomData,
        }
    }

    pub fn into_inner(this: Node<T>) -> T {
        unsafe {
            let data = ptr::read(this.inner.as_ref().data.as_ptr());
            drop(Box::from_raw(this.inner.as_ptr()));
            mem::forget(this);
            data
        }
    }
}

impl<T> Deref for Node<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner.as_ref().data.as_ptr() }
    }
}

impl<T> DerefMut for Node<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inner.as_mut().data.as_mut_ptr() }
    }
}

impl<T> Drop for Node<T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.inner.as_mut().data.as_mut_ptr());
            drop(Box::from_raw(self.inner.as_ptr()));
        }
    }
}

pub struct Queue<T> {
    head: Cell<*mut NodeInner<T>>,
    phantom: PhantomData<T>,
}

unsafe impl<T: Send> Send for Queue<T> {}

impl<T> Queue<T> {
    pub fn new() -> Queue<T> {
        let node = Box::into_raw(Box::new(NodeInner {
            next: AtomicPtr::new(ptr::null_mut()),
            data: MaybeUninit::uninit(),
        }));

        Queue { head: Cell::new(node), phantom: PhantomData }
    }

    pub fn split(self) -> (Producer<T>, Consumer<T>) {
        let queue = Arc::new(self);

        let producer = Producer { queue: queue.clone(), tail: queue.head.get() };
        let consumer = Consumer { queue };

        (producer, consumer)
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        unsafe {
            let head = self.head.get();
            let mut current = (*head).next.load(Ordering::Relaxed);

            drop(Box::from_raw(head));

            while !current.is_null() {
                let next = (*current).next.load(Ordering::Relaxed);
                ptr::drop_in_place((*current).data.as_mut_ptr());
                drop(Box::from_raw(current));
                current = next;
            }
        }
    }
}

pub struct Consumer<T> {
    queue: Arc<Queue<T>>,
}

unsafe impl<T: Send> Send for Consumer<T> {}

impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<Node<T>> {
        unsafe {
            let head = self.queue.head.get();
            let next = (*head).next.load(Ordering::Acquire);

            if !next.is_null() {
                ptr::copy_nonoverlapping((*next).data.as_ptr(), (*head).data.as_mut_ptr(), 1);
                (*head).next.store(ptr::null_mut(), Ordering::Relaxed);

                self.queue.head.set(next);

                return Some(Node { inner: NonNull::new_unchecked(head), phantom: PhantomData });
            }

            None
        }
    }
}

pub struct Producer<T> {
    #[allow(unused)]
    queue: Arc<Queue<T>>,
    tail: *mut NodeInner<T>,
}

unsafe impl<T: Send> Send for Producer<T> {}

impl<T> Producer<T> {
    pub fn push(&mut self, node: Node<T>) {
        unsafe {
            let node_ptr = node.inner.as_ptr();
            mem::forget(node);

            let tail = &*self.tail;
            tail.next.store(node_ptr, Ordering::Release);

            self.tail = node_ptr;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    #[test]
    fn multithreaded() {
        let (mut producer, mut consumer) = Queue::new().split();

        let thread1 = std::thread::spawn(move || {
            for _ in 0..10000 {
                producer.push(Node::new(false));
            }

            producer.push(Node::new(true));
        });

        let thread2 = std::thread::spawn(move || {
            let mut counter = 0;

            loop {
                if let Some(node) = consumer.pop() {
                    if *node {
                        break;
                    } else {
                        counter += 1;
                    }
                }
            }

            assert_eq!(counter, 10000);
        });

        thread1.join().unwrap();
        thread2.join().unwrap();
    }

    #[test]
    fn multiple_queues() {
        let (mut producer1, mut consumer1) = Queue::new().split();
        let (mut producer2, mut consumer2) = Queue::new().split();

        for _ in 0..10000 {
            producer1.push(Node::new(()));
        }

        let mut counter = 0;
        while let Some(node) = consumer1.pop() {
            producer2.push(node);
            counter += 1;
        }
        assert_eq!(counter, 10000);

        let mut counter = 0;
        while let Some(_) = consumer2.pop() {
            counter += 1;
        }
        assert_eq!(counter, 10000);
    }

    #[test]
    fn drop_occurs() {
        struct S(Arc<Cell<usize>>);

        impl Drop for S {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let (mut producer, mut consumer) = Queue::new().split();

        let counter = Arc::new(Cell::new(0));

        for _ in 0..10000 {
            producer.push(Node::new(S(counter.clone())));
        }

        while let Some(_) = consumer.pop() {}

        assert_eq!(counter.get(), 10000);
    }
}
