# ch4-T3L4 架构总览

## 1. 目标与范围

`tg-rcore-tutorial-ch4-T3L4` 是在 chapter 4 地址空间机制上继续扩展的教学内核，目标是让用户态单人俄罗斯方块运行在独立地址空间中，并且完成 framebuffer、输入与设备访问在 Sv39 下的正确接线。

整体目标分成两部分：

- **内核侧**：保留 chapter 4 的多进程/地址空间主线，补齐图形、输入和设备 MMIO 映射，提供 framebuffer 虚拟地址、flush 与输入模式切换 syscall；
- **用户侧**：实现 `tetris.rs`，完成方块旋转、行消除、计分、速度递增，并使用增量渲染降低 framebuffer 写入量。

本 crate 仍为 `no_std`、`no_main` 的 RISC-V S-mode 教学内核。

## 2. 总体模块结构

### 2.1 内核代码

- `src/main.rs`
      - 内核入口 `_start` 与 `rust_main`；
      - 建立内核地址空间并启用 Sv39；
      - 进程装载、调度与 trap 分发；
      - syscall 分发与地址翻译；
      - 图形与输入子系统统一接线。
- `src/process.rs`
      - `Process`：用户上下文、地址空间、堆边界与 syscall 计数；
      - `Process::new()`：解析 ELF 并映射用户段与用户栈；
      - `change_program_brk()`：实现 `sbrk`。
- `src/gpu.rs`
      - VirtIO GPU 初始化；
      - framebuffer 地址、长度、宽高导出；
      - framebuffer flush。
- `src/uart.rs`
      - UART 初始化、非阻塞读取、接收中断开关。
- `src/plic.rs`
      - PLIC priority / enable / threshold / claim / complete。
- `build.rs`
      - 解析 `tg-rcore-tutorial-user-T3L4/cases.toml`；
      - 构建用户程序并生成 `APP_ASM`；
      - 写入链接脚本。

### 2.2 用户代码

- `tg-rcore-tutorial-user-T3L3/src/lib.rs`
      - 提供 `framebuffer_info()` / `framebuffer_flush()`；
      - 提供 `getchar_poll()` / `getchar_blocking()`；
      - 通过 syscall 与内核图形、输入能力连接。
- `tg-rcore-tutorial-user-T3L3/src/bin/tetris.rs`
      - 实现俄罗斯方块玩法；
      - 支持轮询/中断式输入模式切换；
      - 直接写 framebuffer，并进行增量渲染。

## 3. 内核侧实现流程

### 3.1 启动与装载

1. `rust_main` 启动后清零 BSS，初始化 console 和 syscall 分发器；
2. 初始化图形与输入子系统；
3. 根据 `tg_linker::AppMeta::locate()` 读取打包进镜像的用户程序；
4. 为每个 app 创建 `Process`，并放入全局进程表；
5. 创建调度线程并进入循环调度。

### 3.2 Trap 与调度

主循环在进程返回后按 `scause` 分支：

- `SupervisorTimer`：时间片到期，切换任务；
- `SupervisorExternal`：处理外部中断，主要是 UART 输入；
- `UserEnvCall`：处理 syscall；
- 其他异常：终止进程。

### 3.3 输入子系统

输入侧复用了 chapter 3 的 UART / PLIC 模块：

- `IO::read` 支持 `STDIN`；
- 轮询模式下优先返回缓存字符，否则非阻塞探测 UART；
- 中断模式下通过 PLIC claim/complete + UART RX 中断收集字符；
- 无输入时返回 `-2`，让用户态继续轮询或等待。

### 3.4 图形子系统

图形侧采用 VirtIO GPU + framebuffer：

- `gpu.rs` 负责初始化 GPU 和 framebuffer；
- 内核通过自定义 syscall 把 framebuffer 信息暴露给用户态；
- 用户程序直接写 framebuffer，再调用 flush。

在 chapter 4 地址空间下，framebuffer 必须映射到用户进程的虚拟地址空间中，因此内核额外维护“首次 syscall 时才建立映射”的状态。

## 4. 用户态俄罗斯方块实现流程

### 4.1 游戏状态

`tetris.rs` 维护：

- 棋盘 10×20；
- 当前方块、下一个方块、随机种子；
- 分数、累计消行、等级、退出/结束状态；
- 渲染缓存，用于增量刷新。

### 4.2 输入模式

启动时先让用户选择模式：

- `p`：轮询模式；
- `i`：interrupt-like 模式。

按键映射：

- `A/D`：左右移动；
- `W`：旋转；
- `S`：软降；
- 空格：硬降；
- `Q`：退出。

### 4.3 渲染策略

用户态采用 framebuffer 直写，不使用字符终端：

- 首帧绘制背景、边框和棋盘；
- 后续按“上一帧状态 vs 当前帧状态”比较，只重绘变化格子；
- 模式条、分数条、等级条、结束提示均采用局部增量更新；
- 最后统一调用 `framebuffer_flush()`。

这样可以显著降低每帧写入量，避免整屏重绘导致的性能问题。

## 5. chapter 4 的关键设计点

### 5.1 固定 framebuffer 虚拟地址

用户态拿到的不是物理地址，而是固定虚拟地址：

- 约定 framebuffer 映射到 `0x1000_0000`；
- 第一次调用 framebuffer syscall 时，内核把物理 framebuffer 映射到当前进程的地址空间；
- 后续重复调用只返回信息，不重复建映射。

这样用户程序始终访问同一虚拟地址，避免每次切换进程都改写渲染逻辑。

### 5.2 设备 MMIO 恒等映射

由于 chapter 4 开启了地址空间，内核访问设备寄存器也必须有页表映射：

- UART：`0x1000_0000..0x1000_1000`
- VirtIO MMIO：`0x1000_1000..0x1001_0000`
- PLIC：`0x0c00_0000..0x0c40_0000`

如果不做这一步，调度线程进入后访问设备寄存器会直接触发页错误。

### 5.3 进程级映射状态

`Process` 中增加 framebuffer 是否已经映射的状态位，确保“只有第一次 syscall 时建立映射”。这避免重复映射同一虚拟区间，也避免多次调用 syscall 时重复修改地址空间。

## 6. 用户程序装载策略

`tg-rcore-tutorial-user-T3L4/cases.toml` 中：

- `[ch4]` 仅保留 `tetris`；
- `[ch4_exercise]` 同样仅保留 `tetris`。

因此内核启动后只装载并运行俄罗斯方块应用。

## 7. 关键设计取舍

- 不引入更复杂的图形栈，仍采用 VirtIO GPU + framebuffer 直写；
- 输入侧沿用 UART 非阻塞读取 + PLIC 接线；
- framebuffer 地址采用固定虚拟地址，降低用户态复杂度；
- 用户态渲染采用增量更新，减少 framebuffer 写入量；
- 内核只在首次 framebuffer syscall 时建立映射，保持语义简单明确。

## 8. 可扩展方向

- 把 interrupt-like 模式升级为真正的外设中断驱动；
- 为 framebuffer 增加更细粒度的脏矩形合并；
- 增加更完整的旋转 kick 规则；
- 把输入缓存升级为多字符队列。