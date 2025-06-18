use crate::events::{Event, EventPriority};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::collections::BinaryHeap;
use std::cmp::Reverse;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct EventChannels {
    order_sender: Sender<Event>,
    order_receiver: Receiver<Event>,
    trade_sender: Sender<Event>,
    trade_receiver: Receiver<Event>,
    system_sender: Sender<Event>,
    system_receiver: Receiver<Event>,
    priority_sender: Sender<PriorityEvent>,
    priority_receiver: Receiver<PriorityEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorityEvent {
    event: Event,
    priority: EventPriority,
    sequence: u64,
}

impl PartialOrd for PriorityEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
            .then_with(|| self.sequence.cmp(&other.sequence))
    }
}

impl EventChannels {
    #[inline]
    pub fn new(capacity: usize) -> Self {
        let (order_sender, order_receiver) = bounded(capacity);
        let (trade_sender, trade_receiver) = bounded(capacity);
        let (system_sender, system_receiver) = bounded(capacity);
        let (priority_sender, priority_receiver) = bounded(capacity * 2);
        
        Self {
            order_sender,
            order_receiver,
            trade_sender,
            trade_receiver,
            system_sender,
            system_receiver,
            priority_sender,
            priority_receiver,
        }
    }
    
    #[inline]
    pub fn unlimited() -> Self {
        let (order_sender, order_receiver) = unbounded();
        let (trade_sender, trade_receiver) = unbounded();
        let (system_sender, system_receiver) = unbounded();
        let (priority_sender, priority_receiver) = unbounded();
        
        Self {
            order_sender,
            order_receiver,
            trade_sender,
            trade_receiver,
            system_sender,
            system_receiver,
            priority_sender,
            priority_receiver,
        }
    }
    
    #[inline]
    pub fn order_sender(&self) -> &Sender<Event> {
        &self.order_sender
    }
    
    #[inline]
    pub fn order_receiver(&self) -> &Receiver<Event> {
        &self.order_receiver
    }
    
    #[inline]
    pub fn trade_sender(&self) -> &Sender<Event> {
        &self.trade_sender
    }
    
    #[inline]
    pub fn trade_receiver(&self) -> &Receiver<Event> {
        &self.trade_receiver
    }
    
    #[inline]
    pub fn system_sender(&self) -> &Sender<Event> {
        &self.system_sender
    }
    
    #[inline]
    pub fn system_receiver(&self) -> &Receiver<Event> {
        &self.system_receiver
    }
    
    #[inline]
    pub fn priority_sender(&self) -> &Sender<PriorityEvent> {
        &self.priority_sender
    }
    
    #[inline]
    pub fn priority_receiver(&self) -> &Receiver<PriorityEvent> {
        &self.priority_receiver
    }
    
    #[inline]
    pub fn send_event(&self, event: Event) -> Result<(), crossbeam_channel::SendError<Event>> {
        match &event {
            Event::Order(_) => self.order_sender.send(event),
            Event::Trade(_) => self.trade_sender.send(event),
            Event::System(_) => self.system_sender.send(event),
        }
    }
    
    #[inline]
    pub fn send_priority_event(&self, event: Event, sequence: u64) -> Result<(), crossbeam_channel::SendError<PriorityEvent>> {
        let priority = event.priority();
        let priority_event = PriorityEvent {
            event,
            priority,
            sequence,
        };
        self.priority_sender.send(priority_event)
    }
}

#[derive(Debug)]
pub struct PriorityQueue {
    heap: Arc<Mutex<BinaryHeap<Reverse<PriorityEvent>>>>,
    sequence_counter: Arc<Mutex<u64>>,
}

impl PriorityQueue {
    #[inline]
    pub fn new() -> Self {
        Self {
            heap: Arc::new(Mutex::new(BinaryHeap::new())),
            sequence_counter: Arc::new(Mutex::new(0)),
        }
    }
    
    #[inline]
    pub fn push(&self, event: Event) {
        let mut counter = self.sequence_counter.lock();
        *counter += 1;
        let sequence = *counter;
        drop(counter);
        
        let priority = event.priority();
        let priority_event = PriorityEvent {
            event,
            priority,
            sequence,
        };
        
        self.heap.lock().push(Reverse(priority_event));
    }
    
    #[inline]
    pub fn pop(&self) -> Option<Event> {
        self.heap.lock().pop().map(|Reverse(priority_event)| priority_event.event)
    }
    
    #[inline]
    pub fn len(&self) -> usize {
        self.heap.lock().len()
    }
    
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.heap.lock().is_empty()
    }
    
    #[inline]
    pub fn clear(&self) {
        self.heap.lock().clear();
    }
}

impl Clone for PriorityQueue {
    fn clone(&self) -> Self {
        let heap = {
            let original_heap = self.heap.lock();
            Arc::new(Mutex::new(original_heap.clone()))
        };
        
        let sequence_counter = {
            let original_counter = self.sequence_counter.lock();
            Arc::new(Mutex::new(*original_counter))
        };
        
        Self {
            heap,
            sequence_counter,
        }
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}