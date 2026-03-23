//! GPU 子系统模块。
//!
//! 负责 VirtIO GPU 初始化、framebuffer 信息导出和刷新。

#[cfg(target_arch = "riscv64")]
use core::{cell::UnsafeCell, sync::atomic::{AtomicUsize, Ordering}};

#[cfg(target_arch = "riscv64")]
use virtio_drivers::{Hal, MmioTransport, PhysAddr, VirtAddr, VirtIOGpu, VirtIOHeader};

#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_BASE: usize = 0x1000_1000;
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_STRIDE: usize = 0x1000;
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_COUNT: usize = 8;

#[cfg(target_arch = "riscv64")]
const DMA_PAGE_SIZE: usize = 4096;
#[cfg(target_arch = "riscv64")]
const DMA_POOL_PAGES: usize = 2048;

#[cfg(target_arch = "riscv64")]
#[repr(align(4096))]
struct DmaPool([u8; DMA_POOL_PAGES * DMA_PAGE_SIZE]);

#[cfg(target_arch = "riscv64")]
#[unsafe(link_section = ".bss.uninit")]
static mut DMA_POOL: DmaPool = DmaPool([0; DMA_POOL_PAGES * DMA_PAGE_SIZE]);

#[cfg(target_arch = "riscv64")]
static DMA_NEXT_PAGE: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_arch = "riscv64")]
struct SimpleHal;

#[cfg(target_arch = "riscv64")]
impl Hal for SimpleHal {
    fn dma_alloc(pages: usize) -> PhysAddr {
        if pages == 0 {
            return 0;
        }
        loop {
            let current = DMA_NEXT_PAGE.load(Ordering::Relaxed);
            let next = match current.checked_add(pages) {
                Some(v) => v,
                None => return 0,
            };
            if next > DMA_POOL_PAGES {
                return 0;
            }
            if DMA_NEXT_PAGE
                .compare_exchange(current, next, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                let base = unsafe { core::ptr::addr_of_mut!(DMA_POOL.0) as usize };
                return base + current * DMA_PAGE_SIZE;
            }
        }
    }

    fn dma_dealloc(_paddr: PhysAddr, _pages: usize) -> i32 {
        0
    }

    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        paddr
    }

    fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
        vaddr
    }
}

#[cfg(target_arch = "riscv64")]
struct GpuCell(UnsafeCell<Option<VirtIOGpu<'static, SimpleHal, MmioTransport>>>);

#[cfg(target_arch = "riscv64")]
unsafe impl Sync for GpuCell {}

#[cfg(target_arch = "riscv64")]
static GPU: GpuCell = GpuCell(UnsafeCell::new(None));

#[cfg(target_arch = "riscv64")]
static FRAMEBUFFER_PTR: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_arch = "riscv64")]
static FRAMEBUFFER_LEN: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_arch = "riscv64")]
static FRAMEBUFFER_W: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_arch = "riscv64")]
static FRAMEBUFFER_H: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_arch = "riscv64")]
fn find_virtio_gpu_mmio() -> Option<usize> {
    const VIRTIO_MAGIC: u32 = 0x7472_6976;
    const DEVICE_ID_GPU: u32 = 16;
    for slot in 0..VIRTIO_MMIO_COUNT {
        let base = VIRTIO_MMIO_BASE + slot * VIRTIO_MMIO_STRIDE;
        let magic = unsafe { (base as *const u32).read_volatile() };
        let device_id = unsafe { ((base + 0x008) as *const u32).read_volatile() };
        if magic == VIRTIO_MAGIC && device_id == DEVICE_ID_GPU {
            return Some(base);
        }
    }
    None
}

/// 初始化图形子系统（VirtIO GPU 与 framebuffer）。
pub(crate) fn init_graphics() {
    #[cfg(target_arch = "riscv64")]
    {
        let gpu_mmio = find_virtio_gpu_mmio().expect("virtio-gpu mmio not found");
        let transport = unsafe {
            MmioTransport::new(core::ptr::NonNull::new(gpu_mmio as *mut VirtIOHeader).unwrap())
        }
        .expect("failed to create MmioTransport");
        let mut gpu: VirtIOGpu<'static, SimpleHal, MmioTransport> = unsafe {
            core::mem::transmute(
                VirtIOGpu::<SimpleHal, MmioTransport>::new(transport)
                    .expect("failed to create VirtIOGpu"),
            )
        };
        let (width, height) = gpu
            .resolution()
            .expect("failed to query display resolution");
        let width = width as usize;
        let height = height as usize;
        let (framebuffer_ptr, len) = {
            let framebuffer = gpu
                .setup_framebuffer()
                .expect("failed to setup framebuffer");
            let visible_len = width.saturating_mul(height).saturating_mul(4);
            let len = framebuffer.len().min(visible_len);
            framebuffer[..len].fill(0);
            (framebuffer.as_mut_ptr() as usize, len)
        };
        gpu.flush().expect("failed to flush framebuffer");

        FRAMEBUFFER_PTR.store(framebuffer_ptr, Ordering::Release);
        FRAMEBUFFER_LEN.store(len, Ordering::Release);
        FRAMEBUFFER_W.store(width, Ordering::Release);
        FRAMEBUFFER_H.store(height, Ordering::Release);
        unsafe { *GPU.0.get() = Some(gpu) };
    }
}

/// 返回 framebuffer 信息：基址、可用长度、宽、高。
pub(crate) fn framebuffer_info() -> Option<(usize, usize, usize, usize)> {
    #[cfg(not(target_arch = "riscv64"))]
    {
        None
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ptr = FRAMEBUFFER_PTR.load(Ordering::Acquire);
        let len = FRAMEBUFFER_LEN.load(Ordering::Acquire);
        let width = FRAMEBUFFER_W.load(Ordering::Acquire);
        let height = FRAMEBUFFER_H.load(Ordering::Acquire);
        if ptr == 0 || len == 0 || width == 0 || height == 0 {
            None
        } else {
            Some((ptr, len, width, height))
        }
    }
}

/// 触发 framebuffer flush。
pub(crate) fn framebuffer_flush() -> isize {
    #[cfg(not(target_arch = "riscv64"))]
    {
        -1
    }
    #[cfg(target_arch = "riscv64")]
    {
        let flush_ok = unsafe {
            match &mut *GPU.0.get() {
                Some(gpu) => gpu.flush().is_ok(),
                None => false,
            }
        };
        if flush_ok {
            0
        } else {
            -1
        }
    }
}
