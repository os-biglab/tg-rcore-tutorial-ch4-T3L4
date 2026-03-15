# ch4-T1L2 Exercise 实现报告（trace / mmap / munmap）

本文对应 `exercise.md` 要求，说明当前 `tg-rcore-tutorial-ch4-T1L2` 中练习功能的实现方式与关键设计点。

## 1. 练习目标

本次练习包含三项：

1. 在引入虚存后重写 `trace`，恢复读/写/统计功能；
2. 新增 `mmap`（syscall id 222）匿名映射；
3. 新增 `munmap`（syscall id 215）取消映射。

相对 ch3 的本质变化：用户指针不能直接解引用，必须走页表翻译并检查权限。

---

## 2. 实现落点

- `src/main.rs`：
  - `impl Trace for SyscallContext`（trace 重写）；
  - `impl Memory for SyscallContext`（mmap/munmap）；
  - 在 `schedule()` 中统一记录 syscall 次数。
- `src/process.rs`：
  - `record_syscall/syscall_count`；
  - `address_space` 作为映射与翻译载体。

---

## 3. trace 重写

## 3.1 核心策略

通过 `process.address_space.translate::<u8>(VAddr::new(id), flags)` 完成：

- 地址可见性判断；
- 读写权限判断；
- 虚拟地址到可访问指针的转换。

### 3.2 三种请求的实现

- `trace_request = 0`（读）：
  - 使用 `READABLE = build_flags("U_RV")`；
  - 翻译失败返回 `-1`；成功返回目标字节值。
- `trace_request = 1`（写）：
  - 使用 `WRITABLE = build_flags("U_W_V")`；
  - 翻译失败返回 `-1`；成功写入 `data as u8` 并返回 `0`。
- `trace_request = 2`（统计）：
  - 返回 `process.syscall_count(id)`。

### 3.3 “本次调用计入统计”的保证

在 `schedule()` 处理 `UserEnvCall` 时：

1. 先从 `a7` 取 syscall id；
2. 调用 `process.record_syscall(id.0)` 记数；
3. 再进入 `tg_syscall::handle(...)` 执行 `trace`。

因此当 `trace_request=2` 查询时，当前这次调用已经计入。

---

## 4. mmap 实现

函数签名：

```rust
fn mmap(&self, caller: Caller, addr: usize, len: usize, prot: i32, _flags: i32, _fd: i32, _offset: usize) -> isize
```

## 4.1 参数与合法性检查

当前实现覆盖了题目要求的主要错误场景：

- `addr` 必须页对齐；
- `prot` 仅允许低 3 位，且不能全 0；
- `addr + len` 使用 `checked_add` 防溢出；
- 目标映射区间不得与已有 `areas` 重叠。

零长度映射按成功处理（返回 `0`）。

## 4.2 权限构造与映射

- 初始 flags 模板：`"U___V"`；
- 按 `prot` 映射到 `X/W/R` 位；
- 使用 `address_space.map(range, &[], 0, flags)` 建立匿名页映射。

这满足题目“按页向上取整映射、不要求指定物理位置”的简化约束。

---

## 5. munmap 实现

函数签名：

```rust
fn munmap(&self, caller: Caller, addr: usize, len: usize) -> isize
```

## 5.1 参数检查

- `addr` 页对齐；
- `addr + len` 防溢出；
- 零长度区间返回 `0`。

## 5.2 映射完整性检查

在调用 `unmap` 前，逐页验证目标区间每个 `vpn` 都已被某个 `area` 覆盖；
若存在未映射页则返回 `-1`，避免部分错误解除映射。

## 5.3 解除映射

验证通过后执行 `process.address_space.unmap(range)`，返回 `0`。

---

## 6. 与 chapter4 框架的集成

- syscall 初始化中启用了 `tg_syscall::init_trace` 与 `init_memory`；
- `Process` 统一维护地址空间与 syscall 计数；
- `trace/mmap/munmap` 无需改动调度事件模型，作为普通 syscall 返回 `Done` 分支。

这样既满足练习目标，也保持了 chapter4 既有执行路径不变。

---

## 7. 验证方式

在 `tg-rcore-tutorial-ch4-T1L2` 目录运行：

```bash
cargo run --features exercise
```

或：

```bash
./test.sh exercise
```

重点关注：

- 非法读写地址时 `trace` 返回 `-1`；
- `mmap` 在非法参数/重叠映射时返回 `-1`；
- `munmap` 在区间含未映射页时返回 `-1`；
- exercise 测例整体通过。

---

## 8. 已知边界

当前实现遵循练习“最小可用”原则，仍有教学化简：

- 未实现高级回收策略与失败回滚；
- 仅支持匿名映射语义（忽略文件参数）；
- 错误处理优先保证语义正确，不追求复杂优化。

这些边界与 chapter4 目标一致，可在后续章节继续演进。