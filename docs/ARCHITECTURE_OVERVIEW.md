# ch4-T1L2 软件架构总览

本文描述 `tg-rcore-tutorial-ch4-T1L2` 作为独立 crate 的实现结构、执行路径与模块分工。

## 1. 系统定位

`ch4-T1L2` 是一个运行在 RISC-V S 态的 `no_std` 裸机内核样例，核心目标是把第三章“多任务调度”升级为“带 Sv39 地址空间隔离的进程系统”。

该 crate 的关键能力：

- 为每个用户进程建立独立地址空间（Sv39）；
- 通过 `ForeignContext + MultislotPortal` 支持跨地址空间上下文切换；
- 基于 ELF `LOAD` 段完成用户程序装载；
- 在 syscall 层对用户指针执行地址翻译与权限检查；
- 支持基础内存管理 syscall（`sbrk`，以及 exercise 中的 `mmap/munmap`）。

与 ch3 的核心差异：ch3 直接把用户地址当内核可访问地址，而 ch4 必须先通过页表翻译。

---

## 2. 目录与模块职责

```text
tg-rcore-tutorial-ch4-T1L2/
├── .cargo/config.toml         # 目标平台、QEMU runner、tg-user 配置
├── build.rs                   # 生成 linker.ld，构建用户程序并生成 APP_ASM
├── Cargo.toml                 # crate 元信息、feature 与依赖
├── README.md                  # 章节说明文档
├── exercise.md                # 练习要求（trace 重写 + mmap/munmap）
└── src/
    ├── main.rs                # 启动、内核地址空间、调度、syscall 实现
    └── process.rs             # 进程结构、ELF 加载、堆管理、syscall 计数
```

---

## 3. 分层架构

```text
用户程序（ELF + ecall）
      │
      ▼
系统调用语义层（main.rs::impls::SyscallContext）
      │
      ▼
进程/地址空间层（process.rs::Process + AddressSpace）
      │
      ▼
页表/上下文切换层（tg-kernel-vm + tg-kernel-context/foreign）
      │
      ▼
RISC-V Sv39 + QEMU virt
```

### 3.1 `main.rs`：系统编排与运行时入口

负责系统级流程：

1. `rust_main` 初始化 BSS、控制台、内核堆；
2. 建立内核地址空间（内核段恒等映射 + 堆映射 + 传送门映射）；
3. 解析内置用户 ELF，创建 `Process` 并挂入全局进程表；
4. 启动调度线程 `schedule`；
5. 调度线程循环执行进程，处理 `ecall` 与异常，直到进程全部退出；
6. 最终 `shutdown(false)`。

### 3.2 `process.rs`：进程对象与地址空间对象

`Process` 聚合了：

- `ForeignContext`（用户上下文 + `satp`）；
- `AddressSpace<Sv39, Sv39Manager>`（独立页表）；
- 堆边界（`heap_bottom/program_brk`）；
- 每进程 syscall 计数。

关键方法：

- `Process::new(elf)`：校验 ELF，映射 `LOAD` 段，映射用户栈，构造 `satp`；
- `change_program_brk(size)`：实现 `sbrk` 堆扩展/回收；
- `record_syscall/syscall_count`：支持 trace 查询。

### 3.3 `impls`：syscall 与 VM 桥接层

通过 `translate()` 把用户虚拟地址转换成内核可访问指针，并按 `VmFlags` 做读写权限校验。该层是 ch4 相比 ch3 的最大变化点。

---

## 4. 启动与执行路径

### 4.1 构建期（build-time）

- `build.rs` 生成 `linker.ld`；
- 构建用户程序并生成 `app.asm`；
- `global_asm!(APP_ASM)` 将用户程序内嵌进内核镜像。

### 4.2 运行期（run-time）

1. `_start` 设栈后跳到 `rust_main`；
2. `kernel_space()` 建立并激活内核页表；
3. 每个 app 由 `Process::new()` 变成独立进程；
4. `schedule()` 通过 `MultislotPortal` 进入用户地址空间执行；
5. trap 返回后处理 syscall/异常并继续调度。

---

## 5. 虚存设计关键点

### 5.1 内核地址空间

- 内核段采用恒等映射（便于访问物理内存）；
- 堆区域映射为可读写；
- 传送门映射到最高虚页 `VPN::MAX`，内核与用户共享同一虚拟位置。

### 5.2 用户地址空间

- ELF `LOAD` 段按段权限映射（`X/W/R` + `U` + `V`）；
- 用户栈映射到高地址固定区间（2 页）；
- 进程堆从 `heap_bottom` 起，通过 `sbrk` 动态调整。

### 5.3 syscall 指针访问

- `write/clock_gettime/trace` 等 syscall 均先 `translate`；
- 未翻译成功或权限不满足时返回 `-1`；
- 避免跨地址空间直接解引用用户虚拟地址。

---

## 6. 配置与依赖

主要外部组件：

- `tg-kernel-vm`：页表与地址空间抽象；
- `tg-kernel-context`（`foreign`）：跨地址空间上下文切换；
- `tg-kernel-alloc`：内核堆分配；
- `tg-syscall`：syscall 统一分发。

---

## 7. 当前实现边界

为了教学最小闭环，当前实现不包含：

- 复杂内存回收策略与碎片整理；
- copy-on-write、按需分页等高级 VM 特性；
- 多核并行调度；
- 完整 POSIX 兼容内存语义。

现有架构已足够支撑 chapter4 的地址空间、trace 重写、mmap/munmap 练习目标。