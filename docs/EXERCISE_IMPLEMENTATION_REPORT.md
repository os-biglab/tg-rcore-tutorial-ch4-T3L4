# ch4-T3L4 增量设计实现报告

本文记录 `tg-rcore-tutorial-ch4-T3L4` 中“让用户态俄罗斯方块运行在地址空间下”的实现过程，重点说明内核侧与用户侧分别做了什么，以及中途遇到的问题和修复方式。

## 1. 任务目标回顾

本次实现目标如下：

1. 在 chapter 4 地址空间框架下运行用户态单人俄罗斯方块；
2. 用户态支持方块旋转、行消除、计分、速度递增；
3. framebuffer 采用固定用户虚拟地址，并由内核第一次 syscall 时建立映射；
4. 将 chapter 3 已完成的 GPU / UART / PLIC 逻辑迁移到 chapter 4；
5. 用户态采用增量渲染，减少 framebuffer 写入。

与 chapter 3 最大的不同是：chapter 4 开启了 Sv39 地址空间，设备访问、framebuffer 访问和用户指针访问都必须经过页表和映射管理。

## 2. 修改清单

### 2.1 内核侧：图形与输入能力迁移

- 将 `gpu.rs`、`uart.rs`、`plic.rs` 接入 `ch4-T3L4/src/main.rs`；
- 初始化 GPU 后导出 framebuffer 信息和 flush；
- 支持 UART 非阻塞读取和 PLIC 中断路由；
- 增加三个自定义 syscall：
  - `0x1000_0001`：framebuffer info；
  - `0x1000_0002`：framebuffer flush；
  - `0x1000_0003`：输入模式切换。

### 2.2 内核侧：地址空间下的 framebuffer 映射

- 在 `Process` 中增加 framebuffer 映射状态位 `fb_mapped`；
- 第一次调用 framebuffer syscall 时，把物理 framebuffer 映射到用户虚拟地址 `0x1000_0000`；
- 后续调用只返回信息，不再重复映射；
- 为避免页错误，内核地址空间显式映射 UART / VirtIO / PLIC 的 MMIO 区域。

### 2.3 用户侧：tetris 程序

- 新增 `user-T3L3/src/bin/tetris.rs`；
- 支持旋转、左右移动、软降、硬降；
- 支持消行、计分、等级提升和速度递增；
- 支持轮询 / interrupt-like 两种输入模式；
- 渲染改为 framebuffer 直写，并做增量刷新。

## 3. 实现过程

### 3.1 先把 chapter 3 的外设能力迁移过来

用户态俄罗斯方块依赖图形和输入，所以先把 chapter 3 中已验证过的能力移到 chapter 4：

- `gpu.rs`：VirtIO GPU 初始化与 framebuffer 维护；
- `uart.rs`：UART 非阻塞读取；
- `plic.rs`：UART 中断的 claim/complete 流程。

迁移后，内核在启动时会先初始化图形与输入，再继续加载用户程序。

### 3.2 再把 framebuffer 接到进程地址空间

chapter 4 的用户程序不能再直接使用物理地址，所以 framebuffer syscall 采用“固定虚拟地址 + 首次映射”的方式：

- 用户态始终把 framebuffer 看成 `0x1000_0000`；
- 内核在第一次 `framebuffer_info()` 时，检查当前进程是否已经映射；
- 如果没有，则把物理 framebuffer 映射进当前进程页表；
- 之后用户程序可以一直使用这个固定虚拟地址画图。

这样做的好处是：用户态渲染代码和 chapter 3 保持一致，只是底层从“物理地址”变成了“固定虚拟地址”。

### 3.3 最后完成用户态 tetris

`tetris.rs` 中实现了：

- 7 种方块的形状表；
- `W` 旋转与简单 wall-kick；
- 行消除和分数计算；
- 速度随等级提升而增加；
- `p/i` 两种输入模式；
- framebuffer 的增量渲染。

渲染部分使用上一帧状态缓存：只在当前帧与上一帧不一致时才重绘对应格子或 HUD 区域。

## 4. 实现中遇到的问题与修复

### 4.1 开启 Sv39 后出现 StorePageFault

一开始运行时，调度线程报了 `StorePageFault`，`stval` 落在 `0x1000_xxxx` 一带。

原因不是内存不够，而是：

- 内核启用了地址空间；
- 但 UART / VirtIO / PLIC 这些设备 MMIO 地址没有被映射到内核页表；
- 结果内核访问设备寄存器时直接页故障。

修复方式：

- 在 `kernel_space()` 里补上设备 MMIO 恒等映射；
- 重点映射 UART、VirtIO MMIO 和 PLIC 区域；
- 重新编译后页故障消失。

### 4.2 framebuffer 地址必须是用户虚拟地址

chapter 3 里用户态直接拿 framebuffer 物理地址就能用，但 chapter 4 开启地址空间后不能这样做。

修复方式：

- 将 framebuffer syscall 扩展为返回固定虚拟地址 `0x1000_0000`；
- 由内核把物理 framebuffer 映射到该虚拟地址；
- 用户态只认这个虚拟地址，不再关心物理地址。

### 4.3 防止重复映射

如果 framebuffer syscall 每次都重新映射，容易污染进程地址空间，也没有必要。

修复方式：

- 在 `Process` 中增加 `fb_mapped` 标志；
- 只有第一次 syscall 建立映射；
- 后续调用只返回 framebuffer 信息。

### 4.4 用户态渲染太重

俄罗斯方块如果每帧整块重画，写 framebuffer 的代价较高。

修复方式：

- 加入 `RenderState`；
- 首帧全量绘制；
- 后续只重绘变化的棋盘格、模式条、分数条、等级条和结束遮罩；
- 最后统一 flush。

## 5. 验证结果

已完成并验证：

- `ch4-T3L4` `cargo build` 通过；
- 用户态 `tetris` 能被内核打包并装载；
- framebuffer syscall 在地址空间下可用；
- MMIO 映射后设备访问不再触发页错误。

## 6. 后续优化方向

- 进一步优化渲染，把多个变化格子合并成脏矩形；
- 增强旋转 kick 规则，提升贴墙旋转体验；
- 把中断式输入升级成更完整的事件队列。