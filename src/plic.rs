//! PLIC 驱动（Platform-Level Interrupt Controller）。
//!
//! 基于 QEMU virt 的 PLIC MMIO 布局，提供：
//! - 中断优先级配置；
//! - 中断使能配置；
//! - 阈值配置；
//! - claim/complete 流程。

/// 中断目标优先级域。
#[derive(Copy, Clone)]
pub(crate) enum IntrTargetPriority {
    /// M 态目标。
    Machine = 0,
    /// S 态目标。
    Supervisor = 1,
}

impl IntrTargetPriority {
    #[inline]
    fn supported_number() -> usize {
        2
    }
}

/// PLIC 设备抽象。
pub(crate) struct Plic {
    base_addr: usize,
}

impl Plic {
    #[inline]
    fn priority_ptr(&self, intr_source_id: usize) -> *mut u32 {
        assert!(intr_source_id > 0 && intr_source_id <= 132);
        (self.base_addr + intr_source_id * 4) as *mut u32
    }

    #[inline]
    fn hart_id_with_priority(hart_id: usize, target_priority: IntrTargetPriority) -> usize {
        hart_id * IntrTargetPriority::supported_number() + target_priority as usize
    }

    #[inline]
    fn enable_ptr(
        &self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        intr_source_id: usize,
    ) -> (*mut u32, usize) {
        let id = Self::hart_id_with_priority(hart_id, target_priority);
        let (reg_id, reg_shift) = (intr_source_id / 32, intr_source_id % 32);
        (
            (self.base_addr + 0x2000 + 0x80 * id + 0x4 * reg_id) as *mut u32,
            reg_shift,
        )
    }

    #[inline]
    fn threshold_ptr(
        &self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
    ) -> *mut u32 {
        let id = Self::hart_id_with_priority(hart_id, target_priority);
        (self.base_addr + 0x20_0000 + 0x1000 * id) as *mut u32
    }

    #[inline]
    fn claim_complete_ptr(
        &self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
    ) -> *mut u32 {
        let id = Self::hart_id_with_priority(hart_id, target_priority);
        (self.base_addr + 0x20_0004 + 0x1000 * id) as *mut u32
    }

    /// 构造 PLIC 访问对象。
    pub(crate) unsafe fn new(base_addr: usize) -> Self {
        Self { base_addr }
    }

    /// 设置中断源优先级（0..=7）。
    pub(crate) fn set_priority(&mut self, intr_source_id: usize, priority: u32) {
        assert!(priority < 8);
        unsafe { self.priority_ptr(intr_source_id).write_volatile(priority) }
    }

    /// 为指定 hart/目标域启用中断源。
    pub(crate) fn enable(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        intr_source_id: usize,
    ) {
        let (reg_ptr, shift) = self.enable_ptr(hart_id, target_priority, intr_source_id);
        unsafe {
            reg_ptr.write_volatile(reg_ptr.read_volatile() | (1u32 << shift));
        }
    }

    /// 设置阈值（0..=7）。
    pub(crate) fn set_threshold(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        threshold: u32,
    ) {
        assert!(threshold < 8);
        unsafe { self.threshold_ptr(hart_id, target_priority).write_volatile(threshold) }
    }

    /// claim 一个中断。
    pub(crate) fn claim(&mut self, hart_id: usize, target_priority: IntrTargetPriority) -> u32 {
        unsafe { self.claim_complete_ptr(hart_id, target_priority).read_volatile() }
    }

    /// complete 一个中断。
    pub(crate) fn complete(
        &mut self,
        hart_id: usize,
        target_priority: IntrTargetPriority,
        completion: u32,
    ) {
        unsafe {
            self.claim_complete_ptr(hart_id, target_priority)
                .write_volatile(completion)
        }
    }
}
