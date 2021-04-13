use std::net::{TcpListener, TcpStream};
use std::io::prelude::*;
use std::fs;
use std::thread;
use std::sync::{mpsc, Arc, Mutex};

/**
 * Other Server Optimizations:
 * - fork/join model.
 * - single-threaded async I/O model.
 */

type Job = Box<dyn FnOnce() + Send + 'static>;
enum Message {
    NewJob(Job),
    Terminate,
}
struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Message>,
}
impl ThreadPool {
    // the number of elements in a collection of threads.
    /// Create a new ThreadPool. 
    /// 
    /// The size is the number of threads in the pool. 
    /// 
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    fn new(size: usize) -> ThreadPool {
        assert!(size > 0);
        let (sender, receiver) = mpsc::channel();

        let receiver = Arc::new(Mutex::new(receiver));

        // preallocates space in the vector (more effecient than *Vec::new*).
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }
        ThreadPool { workers, sender }
    }
    fn execute<F>(&self, f: F) 
        // the lifetime would be the same as the whole app.
        where F: FnOnce() -> () + Send + 'static, {
            let job = Box::new(f);
            self.sender.send(Message::NewJob(job)).unwrap();
        }
}
impl Drop for ThreadPool {
    fn drop(&mut self) {
        println!("Sending terminate message to all workers.");
        for _ in &self.workers {
            self.sender.send(Message::Terminate).unwrap();
        }
        println!("Shutting down all workers.");
        for worker in &mut (self.workers) {
            println!("Shutting down worker {}", worker.id);
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}
impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            // acquire the mutex first, and then block here waiting for a job.
            // the ownership of the lock is based on the lifetime of the "MutexGuard<T>" that the method returns.
            let message = receiver.lock().unwrap().recv().unwrap();
            match message {
                Message::NewJob(job) => {
                    println!("Worker {} got a job; executing.", id);
                    job();
                }
                Message::Terminate => {
                    println!("Worker {} was told to terminate.", id);
                    break;
                }
            }
        });
        Worker { id, thread: Some(thread) }
    }
}

fn main() {
    // listen for TCP connections.
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(4);

    // returns an iterator that gives us a sequence of streams [TcpStream].
    // process each connection in turn and produce a series of streams for us to handle.
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        pool.execute(|| {
            handle_connection(stream);
        });
    }
}

/**
 * HTTP Response Format:
 * 
 * HTTP-Version Status-Code Reason-Phrase CRLF
 * headers CRLF
 * message-body
 * 
 * .e.g. HTTP/1.1 200 OK\r\n\r\n.
 * - CRLF stands for carriage return and line feed (\r\n).
 */

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).unwrap();

    // byte string.
    let get = b"GET / HTTP/1.1\r\n";

    let (status_line, filename) = if buffer.starts_with(get) {
        // return tuple instead.
        ("HTTP/1.1 200 OK\r\n\r\n", "hello.html")
    } else {
        ("HTTP/1.1 404 NOT FOUND\r\n\r\n", "404.html")
    };

    let contents = fs::read_to_string(filename).unwrap();
    let response = format!("{}{}", status_line, contents);
    // convert the string to bytes, and sends those bytes directly down the connection.
    stream.write(response.as_bytes()).unwrap();
    // flush the internal buffer of "TcpStream".
    stream.flush().unwrap();
}
