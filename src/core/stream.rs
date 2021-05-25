use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use super::source::{new_source, source_get, SourceFuncs};

use glib::{MainContext, Source, SourceId};

type Callback<MSG> = Rc<RefCell<Option<Box<dyn FnMut(MSG)>>>>;

/// Data for the `EventStream` struct.
struct EventStreamData<MSG> {
    events: VecDeque<MSG>,
    locked: bool,
    // We use an Rc here to be able to clone the function to call it so that we don't borrow the
    // stream while calling the function. Otherwise, calling an observer could trigger a
    // borrow_mut() which would result in a panic.
    observers: Vec<Rc<dyn Fn(&MSG)>>,
}

impl<MSG> SourceFuncs for SourceData<MSG> {
    fn dispatch(&self) -> bool {
        let event = self.stream.borrow_mut().events.pop_front();
        if let (Some(event), Some(callback)) = (event, self.callback.borrow_mut().as_mut()) {
            callback(event);
        }
        true
    }

    fn prepare(&self) -> (bool, Option<u32>) {
        (!self.stream.borrow().events.is_empty(), None)
    }
}

struct SourceData<MSG> {
    callback: Callback<MSG>,
    stream: Rc<RefCell<EventStreamData<MSG>>>,
}

fn emit<MSG>(stream: &Rc<RefCell<EventStreamData<MSG>>>, msg: MSG) {
    if !stream.borrow().locked {
        let len = stream.borrow().observers.len();
        for i in 0..len {
            let observer = stream.borrow().observers[i].clone();
            observer(&msg);
        }

        stream.borrow_mut().events.push_back(msg);
    }
}

/// A stream of messages to be used for widget/signal communication and inter-widget communication.
/// EventStream cannot be send to another thread. Use a `Channel` `Sender` instead.
pub struct EventStream<MSG> {
    source: Source,
    source_id: Option<SourceId>,
    _phantom: PhantomData<*mut MSG>,
}

impl<MSG> Drop for EventStream<MSG> {
    fn drop(&mut self) {
        // Ignore error since we're in a destructor.
        let _ = Source::remove(self.source_id.take().expect("source id"));
        self.close();
    }
}

impl<MSG> EventStream<MSG> {
    fn get_callback(&self) -> Callback<MSG> {
        source_get::<SourceData<MSG>>(&self.source).callback.clone()
    }

    fn get_stream(&self) -> &Rc<RefCell<EventStreamData<MSG>>> {
        &source_get::<SourceData<MSG>>(&self.source).stream
    }
}

impl<MSG> EventStream<MSG> {
    /// Create a new event stream.
    pub fn new() -> Self {
        let event_stream: EventStreamData<MSG> = EventStreamData {
            events: VecDeque::new(),
            locked: false,
            observers: Vec::new(),
        };
        let source = new_source(SourceData {
            callback: Rc::new(RefCell::new(None)),
            stream: Rc::new(RefCell::new(event_stream)),
        });
        let main_context = MainContext::default();
        let source_id = Some(source.attach(Some(&main_context)));
        EventStream {
            source,
            source_id,
            _phantom: PhantomData,
        }
    }

    /// Close the event stream, i.e. stop processing messages.
    pub fn close(&self) {
        self.source.destroy();
    }

    /// Synonym for downgrade().
    pub fn stream(&self) -> StreamHandle<MSG> {
        self.downgrade()
    }

    /// Create a Clone-able EventStream handle.
    pub fn downgrade(&self) -> StreamHandle<MSG> {
        StreamHandle::new(Rc::downgrade(self.get_stream()))
    }

    /// Send the `event` message to the stream and the observers.
    pub fn emit(&self, event: MSG) {
        let stream = self.get_stream();
        emit(stream, event)
    }

    /// Lock the stream (don't emit message) until the `Lock` goes out of scope.
    pub fn lock(&self) -> Lock<MSG> {
        let stream = self.get_stream();
        stream.borrow_mut().locked = true;
        Lock {
            stream: self.stream(),
        }
    }

    /// Add an observer to the event stream.
    /// This callback will be called every time a message is emmited.
    pub fn observe<CALLBACK: Fn(&MSG) + 'static>(&self, callback: CALLBACK) {
        let stream = self.get_stream();
        stream.borrow_mut().observers.push(Rc::new(callback));
    }

    /// Add a callback to the event stream.
    /// This is the main callback and received a owned version of the message, in contrast to
    /// observe().
    pub fn set_callback<CALLBACK: FnMut(MSG) + 'static>(&self, callback: CALLBACK) {
        let source_callback = self.get_callback();
        *source_callback.borrow_mut() = Some(Box::new(callback));
    }
}

/// Handle to a EventStream to emit messages.
pub struct StreamHandle<MSG> {
    stream: Weak<RefCell<EventStreamData<MSG>>>,
}

impl<MSG> Clone for StreamHandle<MSG> {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.clone(),
        }
    }
}

impl<MSG> StreamHandle<MSG> {
    fn new(stream: Weak<RefCell<EventStreamData<MSG>>>) -> Self {
        Self { stream }
    }

    /// Same as clone(). Useful for the macro relm_observer_new.
    // TODO#relm4: remove this
    pub fn stream(&self) -> Self {
        self.clone()
    }

    /// Send the `event` message to the stream and the observers.
    pub fn emit(&self, msg: MSG) {
        if let Some(ref stream) = self.stream.upgrade() {
            emit(stream, msg);
        } else {
            panic!("Trying to call emit() on a dropped EventStream");
        }
    }

    /// Lock the stream (don't emit message) until the `Lock` goes out of scope.
    pub fn lock(&self) -> Lock<MSG> {
        if let Some(ref stream) = self.stream.upgrade() {
            stream.borrow_mut().locked = true;
            Lock {
                stream: self.clone(),
            }
        } else {
            panic!("Trying to call lock() on a dropped EventStream");
        }
    }

    fn unlock(&self) {
        if let Some(ref stream) = self.stream.upgrade() {
            stream.borrow_mut().locked = false;
        } else {
            panic!("Trying to call unlock() on a dropped EventStream");
        }
    }

    /// Add an observer to the event stream.
    /// This callback will be called every time a message is emitted.
    pub fn observe<CALLBACK: Fn(&MSG) + 'static>(&self, callback: CALLBACK) {
        if let Some(ref stream) = self.stream.upgrade() {
            stream.borrow_mut().observers.push(Rc::new(callback));
        } else {
            panic!("Trying to call observe() on a dropped EventStream");
        }
    }
}

/// A lock is used to temporarily stop emitting messages on an [`EventStream`].
pub struct Lock<MSG> {
    stream: StreamHandle<MSG>,
}

impl<MSG> Drop for Lock<MSG> {
    fn drop(&mut self) {
        self.stream.unlock();
    }
}
