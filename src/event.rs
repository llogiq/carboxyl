use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::thread::Thread;
use subject::Subject;
use Behaviour;

/// Thread keeps internal subject alive
fn spawn_over<A, B, F>(src: &Event<A>, dest: &Event<B>, f: F)
    where F: Fn(&mut Subject<B>, A) + Send,
          A: Send + Sync + Clone,
          B: Send + Sync + Clone,
{
    let source = src.listen();
    let subject = dest.subject.downgrade();
    let keep_alive = src.subject.clone();
    Thread::spawn(move || {
        // This reference is only here to keep the subject observed by our
        // source alive, as long as this thread is running.
        let _ = keep_alive;
        for a in source.iter() {
            match subject.upgrade() {
                Some(subject) => f(&mut *subject.write().unwrap(), a),
                None => break,
            }
        }
    });
}

pub struct Event<A> {
    subject: Arc<RwLock<Subject<A>>>,
}

impl<A: Send + Sync + Clone> Event<A> {
    pub fn new() -> Event<A> {
        Event { subject: Arc::new(RwLock::new(Subject::new())) }
    }

    pub fn send(&self, a: A) {
        self.subject.write().unwrap().send(a);
    }

    pub fn listen(&self) -> Receiver<A> {
        self.subject.write().unwrap().listen()
    }


    pub fn map<B: Send + Sync + Clone, F: Fn(A) -> B + Send>(&self, f: F) -> Event<B> {
        let event = Event::new();
        spawn_over(self, &event, move |mut subject, a| subject.send(f(a)));
        event
    }

    pub fn filter<F: Fn(&A) -> bool + Send>(&self, f: F) -> Event<A> {
        let event = Event::new();
        spawn_over(self, &event, move |mut subject, a| if f(&a) { subject.send(a) });
        event
    }

    /// Note: the specific order of the merge is not guaranteed to be consistent
    /// with the order, in which they were fired.
    pub fn merge(&self, other: &Event<A>) -> Event<A> {
        let event = Event::new();
        spawn_over(self, &event, |mut subject, a| subject.send(a));
        spawn_over(other, &event, |mut subject, a| subject.send(a));
        event
    }

    pub fn hold(&self, a: A) -> Behaviour<A> {
        Behaviour::new(a, self)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sink() {
        let sink = Event::new();
        let r = sink.listen();
        sink.send(1);
        assert_eq!(r.recv(), Ok(1))
    }

    #[test]
    fn map() {
        let sink = Event::new();
        let triple = sink.map(|x| 3 * x);
        let r = triple.listen();
        sink.send(1);
        assert_eq!(r.recv(), Ok(3));
    }

    #[test]
    fn filter() {
        let sink: Event<i32> = Event::new();
        let positive = sink.filter(|&x| x > 0);
        let r = positive.listen();
        sink.send(-2);
        sink.send(3);
        assert_eq!(r.recv(), Ok(3));
    }

    #[test]
    fn merge() {
        let sink1 = Event::new();
        let sink2 = Event::new();
        let merge = sink1.merge(&sink2);
        let r = merge.listen();
        sink1.send(3);
        sink2.send(4);
        let result = (r.recv(), r.recv());
        // We can not be certain about the ordering
        assert!((result == (Ok(3), Ok(4))) || (result == (Ok(4), Ok(3))));
    }

    #[test]
    fn chain() {
        let sink: Event<i32> = Event::new();
        let chain = sink.map(|x| x + 2).filter(|&x| x > 10);
        let r = chain.listen();
        sink.send(9);
        assert_eq!(r.recv(), Ok(11));
    }

    #[test]
    fn chain_more() {
        let sink: Event<i32> = Event::new();
        let chain = sink
            .map(|x| x + 2)
            .filter(|&x| x > 10)
            .merge(&sink.filter(|&x| x < -3))
            .map(|x| x - 4);
        let r = chain.listen();
        sink.send(9);
        assert_eq!(r.recv(), Ok(7));
        sink.send(-5);
        assert_eq!(r.recv(), Ok(-9));
    }
}
