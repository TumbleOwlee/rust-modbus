//! Allocation counting: what the buffer-reuse requirements are actually for
//! (FR-R-141, FR-R-143, TR-R-043, NF-R-009).
//!
//! Every other test in this suite asserts on bytes. These assert on the number
//! of times the allocator was called, which is the only way to observe a
//! requirement whose whole content is "and it does not allocate".
//!
//! The counting allocator below is the one place in this repository that needs
//! `unsafe`: [`GlobalAlloc`] cannot be implemented without it. The library
//! itself carries `forbid(unsafe_code)` (NF-R-011) and is unaffected — this is a
//! test harness, not shipped code, and it does nothing but increment a counter
//! before delegating every operation to the system allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::Result as IoResult;
use std::pin::Pin;
use std::task::{Context, Poll};

use rust_modbus::{
    Address, Ascii, FrameTransport, Framing, MbapHeader, Quantity, RequestPdu, Tcp, TransactionId,
    UnitId,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// The allocator the whole test binary runs on.
#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// Allocations seen on this thread while counting was switched on.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    /// Whether this thread is inside a counted region.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

/// A pass-through allocator that counts the calls that hand out memory.
///
/// A reallocation counts: growing a buffer that was reserved too small is
/// exactly the cost FR-R-141 exists to remove, and it is invisible if only
/// fresh allocations are counted.
struct Counting;

// The counters are thread-local `Cell<usize>`s with const initialisers, so
// neither reading nor writing them allocates and the allocator cannot recurse
// into itself.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: the layout is the caller's, passed through unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout are the caller's, passed through
        // unchanged; every pointer we hand out came from `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        // SAFETY: as above.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: as above.
        unsafe { System.alloc_zeroed(layout) }
    }
}

/// Count one allocation, if this thread is inside a counted region.
fn record() {
    let counting = COUNTING.try_with(Cell::get).unwrap_or(false);
    if counting {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get().saturating_add(1)));
    }
}

/// Run `body` with the allocator counting, and report how many allocations it
/// performed.
fn allocations(body: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|on| on.set(true));
    body();
    COUNTING.with(|on| on.set(false));
    ALLOCATIONS.with(Cell::get)
}

/// A stream that swallows every byte written to it and never yields one.
///
/// Nothing here allocates, so what the counter sees during a send is the
/// transport's own behavior and not the test's plumbing.
struct Sink;

impl AsyncWrite for Sink {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for Sink {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        Poll::Pending
    }
}

/// The MBAP header every frame below carries.
fn header() -> MbapHeader {
    MbapHeader {
        transaction_id: TransactionId(1),
        unit_id: UnitId(0x11),
    }
}

/// A request with a body, so the encode does more than push a function code.
fn request() -> RequestPdu {
    RequestPdu::ReadHoldingRegisters {
        address: Address(0x006B),
        quantity: Quantity(3),
    }
}

/// How many frames a steady state is measured over.
const FRAMES: usize = 100;

#[test]
/// FR-R-141, NF-R-009 — a caller that reuses one buffer allocates at most once,
/// however many frames it encodes: the first encode reserves the framing's
/// maximum and every later one writes into capacity that already exists.
fn it_reused_buffer_allocates_once() {
    let mut out = Vec::new();

    let first = allocations(|| {
        Tcp::encode_request_into(&header(), &request(), &mut out).expect("encodes");
    });
    assert!(first <= 1, "the first frame allocated {first} times");

    let rest = allocations(|| {
        for _ in 0..FRAMES {
            out.clear();
            Tcp::encode_request_into(&header(), &request(), &mut out).expect("encodes");
        }
    });
    assert_eq!(rest, 0, "{FRAMES} further frames allocated {rest} times");
}

#[test]
/// FR-R-143 — ASCII is the carve-out: its wire form is a transformation of the
/// binary ADU rather than a wrapping of it, so it may build that binary form in
/// one scratch buffer per frame — one, and no more.
fn it_ascii_encode_allocates_at_most_once_per_frame() {
    let mut out = Vec::new();
    Ascii::encode_request_into(&UnitId(0x11), &request(), &mut out).expect("encodes");

    let counted = allocations(|| {
        for _ in 0..FRAMES {
            out.clear();
            Ascii::encode_request_into(&UnitId(0x11), &request(), &mut out).expect("encodes");
        }
    });
    assert!(
        counted <= FRAMES,
        "{FRAMES} frames allocated {counted} times, more than one scratch each"
    );
}

#[tokio::test]
/// TR-R-043 — a transport encodes into a buffer it owns and reuses, so once it
/// has sent its first frame, sending allocates nothing at all.
async fn it_transport_sending_is_allocation_free_in_steady_state() {
    let mut transport = FrameTransport::<_, Tcp>::new(Sink);
    transport
        .send_request(&header(), &request())
        .await
        .expect("sends");

    let header = header();
    let request = request();
    let mut counted = 0;
    for _ in 0..FRAMES {
        // Counting is switched on around the send itself, so what it sees is
        // the transport's own behavior and not the loop's.
        ALLOCATIONS.with(|count| count.set(0));
        COUNTING.with(|on| on.set(true));
        let result = transport.send_request(&header, &request).await;
        COUNTING.with(|on| on.set(false));
        counted += ALLOCATIONS.with(Cell::get);
        result.expect("sends");
    }
    assert_eq!(counted, 0, "{FRAMES} frames allocated {counted} times");
}
