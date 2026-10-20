//! Rect pipelines built off the render thread. A frame's prepare requests
//! each new variant from resident workers, so the variants it needs compose
//! and compile side by side; its execute waits for one only when it first
//! draws with it. Each worker keeps its own naga_oil `Composer` (see
//! `rect_shader`), which costs about as much to build as a variant does.
//! Without threads (wasm32) a request builds in place.

use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

/// A rect pipeline that may still be building.
#[derive(Clone)]
pub(crate) struct RectPipeline(Arc<Slot>);

struct Slot {
    pipeline: OnceLock<Result<wgpu::RenderPipeline, String>>,
    lock: Mutex<()>,
    ready: Condvar,
}

impl Slot {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pipeline: OnceLock::new(),
            lock: Mutex::new(()),
            ready: Condvar::new(),
        })
    }

    fn fulfill(&self, result: Result<wgpu::RenderPipeline, String>) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = self.pipeline.set(result);
        self.ready.notify_all();
    }
}

impl RectPipeline {
    /// Builds the pipeline on a worker, or in place when none can start.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn request(
        device: &wgpu::Device,
        build: impl FnOnce(&wgpu::Device) -> wgpu::RenderPipeline + Send + 'static,
    ) -> Self {
        let slot = Slot::new();
        let out = slot.clone();
        let device = device.clone();
        let job: workers::Job = Box::new(move || {
            // A build that panics reaches the render thread on first use.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(&device)))
                .map_err(|payload| panic_message(payload.as_ref()));
            out.fulfill(result);
        });
        if let Err(job) = workers::submit(job) {
            job();
        }
        Self(slot)
    }

    /// Builds the pipeline in place: there are no workers without threads.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn request(
        device: &wgpu::Device,
        build: impl FnOnce(&wgpu::Device) -> wgpu::RenderPipeline,
    ) -> Self {
        let slot = Slot::new();
        slot.fulfill(Ok(build(device)));
        Self(slot)
    }

    /// The pipeline, waiting for its build the first time.
    pub(crate) fn get(&self) -> &wgpu::RenderPipeline {
        let slot = &*self.0;
        let result = match slot.pipeline.get() {
            Some(result) => result,
            None => {
                let mut guard = slot.lock.lock().unwrap_or_else(PoisonError::into_inner);
                while slot.pipeline.get().is_none() {
                    guard = slot
                        .ready
                        .wait(guard)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                drop(guard);
                slot.pipeline.get().expect("fulfilled above")
            }
        };
        match result {
            Ok(pipeline) => pipeline,
            Err(message) => panic!("build rect pipeline: {message}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a worker panicked".to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
mod workers {
    use std::sync::{Arc, Mutex, OnceLock, mpsc};

    pub(super) type Job = Box<dyn FnOnce() + Send>;

    static QUEUE: OnceLock<Option<mpsc::Sender<Job>>> = OnceLock::new();

    /// Hands `job` to a resident worker, or gives it back when none can run.
    pub(super) fn submit(job: Job) -> Result<(), Job> {
        match QUEUE.get_or_init(start) {
            Some(sender) => sender.send(job).map_err(|error| error.0),
            None => Err(job),
        }
    }

    fn start() -> Option<mpsc::Sender<Job>> {
        // A frame rarely meets more than a few new variants at once, and the
        // render thread keeps a core for the rest of the frame.
        let count = std::thread::available_parallelism()
            .map_or(1, |cores| cores.get().saturating_sub(1))
            .clamp(1, 4);
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut started = 0;
        for index in 0..count {
            let receiver = receiver.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("rfgui-rect-pipeline-{index}"))
                .spawn(move || {
                    loop {
                        // The lock is held only while waiting for a job.
                        let job = match receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => return,
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => return,
                        }
                    }
                });
            started += usize::from(spawned.is_ok());
        }
        (started > 0).then_some(sender)
    }
}
