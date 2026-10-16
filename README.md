# closetag

本地标签管理系统：**bun + actix-web → 单二进制**。给本地代码库的文件打标签，按标签树 / 分组筛选，启动自动打开页面，关闭页面自动退出。

## 功能

- **多仓库**：每个 repo 独立一个 SQLite 数据库（一个 repo 一个 db）
- **条目（items）**：扫描仓库文件自动收录；ID 由可配置规则从相对路径生成；支持手动增删改
- **标签（tags）**：树形结构（parent_id），可挂到条目上；删除标签时子标签移到根级
- **分组（tag_groups）**：`includes`（null = 全部）+ `excludes` 规则筛选条目，实时统计命中数
- **规则按仓库隔离**：文件后缀与 ID 生成规则每个仓库各一份，保存在该仓库自己的数据库里；config.toml 里的同名项只作为「添加新仓库时」的初始模板
- **配置**：`repo_paths` / `include_subfixs` / `id_gen_fns` / 服务开关，全部可在页面里改
- **单二进制**：前端用 bun 构建后由 rust-embed 嵌入，`cargo build --release` 产出一个 exe
- **启停行为**：启动自动打开浏览器；页面关闭（WebSocket 断开）约 3 秒后自动退出

## 快速开始

### 构建前提

| 工具 | 说明 |
| --- | --- |
| Rust (stable) | 本机默认为 `x86_64-pc-windows-gnu` 工具链 |
| bun | 构建前端 |
| MinGW-w64 工具链 | **仅 gnu 工具链需要**：gcc + binutils（含 `dlltool`），用于链接和编译内置 SQLite |

本机的 MinGW 工具链位于 `D:\dev\tools\w64devkit`（w64devkit 2.10.1）。构建前执行：

```bash
export PATH="/d/dev/tools/w64devkit/bin:$PATH"
```

> w64devkit 2.10.1 的 GCC 16 不再提供 `libgcc_eh.a`，而 rustc 的 gnu target 会传 `-lgcc_eh`。
> 本机已将 `libgcc.a` 复制为同名文件补齐（`lib/gcc/x86_64-w64-mingw32/16.2.0/libgcc_eh.a`）。
> 如果你用 MSVC 工具链（`rustup default stable-x86_64-pc-windows-msvc` + VS Build Tools），则不需要 MinGW。

### 构建与运行

```bash
scripts/build.sh          # = bun install && bun run build && cargo build --release
./target/release/closetag  # 启动并自动打开 http://127.0.0.1:17733
```

Windows PowerShell：

```powershell
scripts\build.ps1
target\release\closetag.exe
```

首次启动会在用户数据目录生成默认 `config.toml`，然后：
1. 到「配置」页添加仓库路径（repo_paths）
2. 到「条目」页点「扫描」收录文件
3. 到「标签」页建标签树，「条目」页给文件打标签

### 命令行参数

```
closetag [--port N] [--no-browser] [--data-dir PATH] [-h|--help]
```

## 配置参考（config.toml）

`include_subfixs` 与 `id_gen_fns` **每个仓库各存一份**，写在各自数据库的 `meta` 表里，
在「配置」页的「当前仓库规则」里编辑。下面这两项是**全局默认值**：
仓库没有自定义规则时使用它们；某个仓库一旦保存过自己的规则，就改为使用自己的那份，
之后全局怎么改都影响不到它（想回头就点「恢复继承全局默认」）。

```toml
repo_paths = ["D:/weiheng/proj/foo"]

include_subfixs = [".ts", ".tsx", ".rs", ".md"]   # 空 = 所有文件

[[id_gen_fns]]
match = "docs/.*"        # 可选：命中才生效的正则
split = "-"              # 切分符，得到 arr
template = "${arr[0]||arr[1]}"   # 取第一个非空段

[server]
port = 17733             # 0 = 自动；被占用时向后尝试 50 个端口
open_browser = true
exit_when_closed = true
```

**id_gen 模板语法**：`${arr[N]}`、`||`（第一个非空）、`+`（拼接）、字符串字面量、括号。
例：`src/utils/date-helper.ts` 按 `-` 切分为 `["src/utils/date", "helper.ts"]`，
模板 `${arr[0]||arr[1]}` 生成 ID `src/utils/date`。所有规则都不命中或渲染为空时，回退为完整相对路径。
配置页提供实时预览。

## 数据存储

按各系统规范放在用户数据目录下，文件夹名 `closetag`：

| 平台 | 位置 |
| --- | --- |
| Windows | `%APPDATA%\closetag`（如 `C:\Users\<你>\AppData\Roaming\closetag`） |
| Linux | `~/.local/share/closetag` |
| macOS | `~/Library/Application Support/closetag` |

```
closetag/
├── config.toml            # 全局配置（含新仓库的默认规则模板）
└── repos/
    ├── <仓库slug>.sqlite   # 一个 repo 一个 db（slug = 文件夹名-路径哈希6位）
    └── ...
```

每个仓库数据库里还有一张 `meta` 表，存放该仓库自己的规则
（`include_subfixs`、`id_gen_fns`，以及扫描时间 `last_scan_at`）。
仓库被移除时数据库文件整体删除，规则随之消失。

## 数据模型

```
tags:      id(int) · name · parent_id → tags(id) [ON DELETE SET NULL] · description
items:     pk(int) · id(string, 由 id_gen 规则生成) · path(相对 repo, UNIQUE)
item_tags: item_pk → items(pk) [CASCADE] · tag_id → tags(id) [CASCADE]
tag_groups: id(int) · name · includes(JSON tag id 数组, null=全部) · excludes(JSON) · description
meta:      key · value   # last_scan_at · include_subfixs · id_gen_fns（本仓库的规则）
```

- **items.id 不唯一**：同一 ID 的多个文件在页面上可按 ID 分组展示（这就是 id_gen 的意义）
- **扫描语义**：新文件插入；ID 变化则更新；磁盘上消失的文件连同标签关联一起删除
- **分组语义**：命中 includes 任一标签（includes 为 null 表示全部）且不命中 excludes 任一标签
- **规则归属**：后缀与 ID 规则存在各 repo 的 `meta` 表，随该 repo 的数据库一起增删；删除 `meta` 中对应键即恢复继承 config.toml 的全局默认

## API 一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET/POST | `/api/repos` | 仓库列表 / 添加（POST body: `{path}`） |
| DELETE | `/api/repos/{id}` | 移除仓库并删除其数据库 |
| POST | `/api/repos/{id}/scan` | 重新扫描仓库 |
| GET/PUT/DELETE | `/api/repos/{id}/settings` | 该仓库自己的后缀 / ID 规则（PUT body: `{include_subfixs, id_gen_fns}`；DELETE = 恢复继承全局默认） |
| GET/POST | `/api/repos/{id}/tags` | 标签列表 / 新建 |
| PUT/DELETE | `/api/repos/{id}/tags/{tag_id}` | 更新 / 删除（子标签移到根级） |
| GET/POST | `/api/repos/{id}/items` | 条目列表（`?tag=&group=&q=&limit=&offset=`）/ 新建 |
| PUT/DELETE | `/api/repos/{id}/items/{pk}` | 更新 / 删除 |
| POST/DELETE | `/api/repos/{id}/items/{pk}/tags[/{tag_id}]` | 批量加标签 / 移除单个标签 |
| GET/POST | `/api/repos/{id}/groups` | 分组列表 / 新建 |
| PUT/DELETE | `/api/repos/{id}/groups/{group_id}` | 更新 / 删除 |
| GET | `/api/repos/{id}/groups/{group_id}/items` | 分组命中条目 |
| GET/PUT | `/api/config` | 读取 / 保存全局配置（含新仓库的默认规则模板） |
| GET | `/api/about` | 版本、数据目录、端口 |
| POST | `/api/idgen/preview` | ID 生成预览（body: `{path, rules?, repo?}`） |
| GET | `/ws` | WebSocket：页面保活；断开即触发退出倒计时 |

## 开发模式

```bash
# 终端 1：后端（会占用配置里的端口）
export PATH="/d/dev/tools/w64devkit/bin:$PATH"
cargo run

# 终端 2：前端 dev server（/api 与 /ws 已配置代理到 127.0.0.1:17733）
cd frontend && bun run dev
```

后端单元测试（idgen 模板求值）在 `src/idgen.rs` 中，需要时手动执行 `cargo test`。

## 已知限制 / 设计取舍

- 扫描会跳过隐藏目录和 `node_modules`、`target`、`dist` 等常见目录（见 `src/scanner.rs` 的 `SKIP_DIRS`）
- 标签/分组数量假设在百级；条目按页加载（默认 100/页），超大仓库扫描为一次性事务
- 「关闭页面自动退出」依赖浏览器 WebSocket：多标签页只剩最后一个关闭时才退出；关页面后 3 秒内重新打开不会退出
- 移除仓库会删除数据库文件，操作前有确认提示
