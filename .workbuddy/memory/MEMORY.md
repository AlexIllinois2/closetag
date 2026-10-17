# closetag 项目长期记忆

## 项目概况
- 本地标签管理系统，位于 `D:/weiheng/proj/closetag`；需求文档 `AGENTS/AGENTS.md`。
- 形态：bun 前端 + actix-web 后端 → 单二进制 `target/release/closetag.exe`（rust-embed 内嵌 `frontend/dist`）。
- 数据：`%APPDATA%\closetag/`（config.toml + repos/<slug>.sqlite，一 repo 一 db）。

## 构建环境（关键）
- 默认工具链 `x86_64-pc-windows-gnu`；**cargo 前必须** `export PATH="/d/dev/tools/w64devkit/bin:$PATH"`（scripts/build.sh 已处理）。
- w64devkit 2.10.1 装在 `D:\dev\tools\w64devkit`；其 GCC 16 缺 `libgcc_eh.a`，已在 `lib/gcc/x86_64-w64-mingw32/16.2.0/` 放了一份 libgcc.a 的副本补齐，**不要动这个目录**。
- msvc 工具链不可用（无 VS Build Tools）。

## 规则归属（重要设计）
- 文件后缀 `include_subfixs` 与 info_parse 规则 `info_parse_fns` **每个仓库各一份**，存在该 repo 自己 sqlite 的 `meta` 表（key 同名，值为 JSON 数组）。代码在 `src/repo_settings.rs`，接口 `GET/PUT/DELETE /api/repos/{id}/settings`。
- `config.toml` 里的同名两项是**全局默认值**：仓库未自定义时继承；保存过自己的规则后就各走各的。
- 添加仓库时**不要固化全局规则的副本**，否则全局默认对该仓库永久失效。详见日志 2026-10-16。
- 2026-10-17 起 `id_gen_fns`（按路径生成字符串 id）已废弃，改为 `info_parse_fns`（按**文件名**解析 name/auth，规则 `{match, split, name, auth}`，代码 `src/infoparse.rs`）；items.id 同时改为自增 int 主键（旧 pk 迁移为新 id），(name, auth) **不建唯一索引**。旧库打开时自动迁移（db.rs `migrate_items_v2` + repo_settings 旧键转换），详见日志 2026-10-17。

## 约定
- 冒烟验证：`scripts/smoke.sh`（28 项端到端，手动运行；不接入构建）。
- 单元测试：`cargo test` 运行，需带 w64devkit PATH。2026-10-17 后分布：infoparse（模板求值+解析）/ config / repo_settings（含旧规则迁移）/ scanner；`cargo test` 前先问用户。
- UI 测试：Edge CDP 裸驱 + release 二进制（Node 22 内置 WebSocket 连 `--remote-debugging-port`）。2026-10-17 新版脚本 `C:\Users\45574\AppData\Local\Temp\ct-uitest2.mjs`（items schema 改版专项 22 断言，全过）；旧全量模板 `ct-uitest.mjs` 已过时（还按 id_gen 写，且跑 vite dev 需另起前端）。headless 下必须用 `textContent` 而非 `innerText`，CDP `returnByValue` 不能返回 DOM 节点。
- 两者都不接入构建流程；用户禁止改完代码自动跑测试，跑之前先问。
- 用户硬性禁令：改完代码禁止自动跑测试、禁止自动构建打包；交付时用文字说明改动与未验证部分。
- 端口默认 17733（占用时自动 +50 内探测）；关闭页面 3s 后自动退出；90s 无页面连接也退出。测试时需另起常驻 WS 保活（node 每 5s 发 ping），否则服务会被自动退出逻辑杀掉；bun 的后台 ws-client 会被回收，用 node 才稳。

## 已知坑
- rust-embed 的 key 是相对 `frontend/dist` 的完整路径（如 `assets/index-xxx.js`）；actix 路由 `/assets/{tail:.*}` 的 tail **不含** `assets/` 前缀，直接拿 tail 去查会 404 导致前端白屏。已改为用 `req.path()` 查（src/routes/assets.rs）。
- 该 bug 已修复并重新构建验证通过（release 二进制 + UI 27/27 + smoke 23/23）。教训：smoke.sh 只测了 `/` 和 favicon，没覆盖 `/assets/*`，所以白屏问题漏到了最后才被发现——冒烟脚本应加一条资源可达断言。
- agent-browser 的 Chromium CDN 下载会超时装不上；用 `--executable-path` 指 Edge 又极慢（新 session 6~16 分钟）。做 UI 自动化优先用「Node 22 内置 WebSocket + Edge CDP（--headless=new --remote-debugging-port）」，几秒就能起。
