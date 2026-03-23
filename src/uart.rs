//! UART（NS16550a）最小驱动。
//!
//! 提供：
//! - 初始化串口基本寄存器；
//! - 配置接收中断开关；
//! - 非阻塞读取一个字节。

const UART_BASE: usize = 0x1000_0000;
const REG_RBR_THR: usize = 0x00;
const REG_IER: usize = 0x01;
const REG_MCR: usize = 0x04;
const REG_LSR: usize = 0x05;

const IER_RX_AVAILABLE: u8 = 1 << 0;
const MCR_DATA_TERMINAL_READY: u8 = 1 << 0;
const MCR_REQUEST_TO_SEND: u8 = 1 << 1;
const MCR_AUX_OUTPUT2: u8 = 1 << 3;
const LSR_DATA_AVAILABLE: u8 = 1 << 0;

/// QEMU virt 平台 UART 的 PLIC 中断号。
pub(crate) const UART_IRQ: u32 = 10;

#[inline]
fn reg_ptr(offset: usize) -> *mut u8 {
    (UART_BASE + offset) as *mut u8
}

#[inline]
fn read_reg(offset: usize) -> u8 {
    unsafe { reg_ptr(offset).cast_const().read_volatile() }
}

#[inline]
fn write_reg(offset: usize, value: u8) {
    unsafe { reg_ptr(offset).write_volatile(value) }
}

/// 初始化 UART 基本工作模式。
pub(crate) fn init() {
    let mcr = MCR_DATA_TERMINAL_READY | MCR_REQUEST_TO_SEND | MCR_AUX_OUTPUT2;
    write_reg(REG_MCR, mcr);
}

/// 配置 UART 接收中断开关。
pub(crate) fn set_rx_interrupt(enable: bool) {
    let value = if enable { IER_RX_AVAILABLE } else { 0 };
    write_reg(REG_IER, value);
}

/// 非阻塞读取一个字节。
///
/// - `Some(byte)`：当前有可读字符；
/// - `None`：当前无输入。
pub(crate) fn read_nonblocking() -> Option<u8> {
    let lsr = read_reg(REG_LSR);
    if lsr & LSR_DATA_AVAILABLE != 0 {
        Some(read_reg(REG_RBR_THR))
    } else {
        None
    }
}
