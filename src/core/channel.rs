use glib::{MainContext, Source};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::mpsc;

use super::source::{new_source, SourceFuncs};

struct ChannelData<MSG> {
    callback: Box<dyn FnMut(MSG)>,
    peeked_value: Option<MSG>,
    receiver: mpsc::Receiver<MSG>,
}

impl<MSG> Clone for Sender<MSG> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// A wrapper over a `std::sync::mpsc::Sender` to wakeup the glib event loop when sending a
/// message.
pub struct Sender<MSG> {
    sender: mpsc::Sender<MSG>,
}

impl<MSG> Sender<MSG> {
    /// Send a message and wakeup the event loop.
    pub fn send(&self, msg: MSG) -> Result<(), mpsc::SendError<MSG>> {
        let result = self.sender.send(msg);
        let context = MainContext::default();
        context.wakeup();
        result
    }
}

/// A channel to send a message to a relm widget from another thread.
pub struct Channel<MSG> {
    _source: Source,
    _phantom: PhantomData<MSG>,
}

impl<MSG> Channel<MSG> {
    /// Create a new channel with a callback that will be called when a message is received.
    pub fn new<CALLBACK: FnMut(MSG) + 'static>(callback: CALLBACK) -> (Self, Sender<MSG>) {
        let (sender, receiver) = mpsc::channel();
        let source = new_source(RefCell::new(ChannelData {
            callback: Box::new(callback),
            peeked_value: None,
            receiver,
        }));
        let main_context = MainContext::default();
        source.attach(Some(&main_context));
        (
            Self {
                _source: source,
                _phantom: PhantomData,
            },
            Sender { sender },
        )
    }
}

impl<MSG> SourceFuncs for RefCell<ChannelData<MSG>> {
    fn dispatch(&self) -> bool {
        // TODO: show errors.
        let msg = self
            .borrow_mut()
            .peeked_value
            .take()
            .or_else(|| self.borrow().receiver.try_recv().ok());
        if let Some(msg) = msg {
            let callback = &mut self.borrow_mut().callback;
            callback(msg);
        }
        true
    }

    fn prepare(&self) -> (bool, Option<u32>) {
        if self.borrow().peeked_value.is_some() {
            return (true, None);
        }
        let peek_val = self.borrow().receiver.try_recv().ok();
        self.borrow_mut().peeked_value = peek_val;
        (self.borrow().peeked_value.is_some(), None)
    }
}
