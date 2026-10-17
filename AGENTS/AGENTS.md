
## 需求
本地标签管理系统 - closetag: bun + actix-web -> 单二进制

存储位置: 用户目录, 按规范来, 命名: closetag

基本配置信息: repo_paths, include_subfixs(筛选文件后缀), info_parse_fns(按文件名解析 name/auth 的规则)

tags: id(int), name, parent_id, description

items: id(int, 自增主键), name, auth, path(相对repo, UNIQUE), tags
  - name/auth 由 info_parse 规则从**文件名(不含后缀)**解析, 用户不手动填写
  - (name, auth) **不建唯一索引**: 不同目录下的同名文件(同一作品的不同变体)可共存, 去重交给使用方
  - 页面可按 name 分组展示; 搜索覆盖 path/name/auth

tag_groups: id(int), name, includes(null: all), excludes(), description

一个repo一个db, 合理生成增删改查逻辑和页面, 命名可自行规范; 启动自动打开页面, 关闭页面自动退出

### info_parse 规则(已取代原 id_gen)
- 规则结构: {match(可选正则, 命中相对路径才生效), split(切分符), name(模板), auth(模板, 可空)}
- 语义: 文件名去后缀后用 split 切成 arr, 分别渲染 name/auth 模板; 按顺序取第一条 name 非空的规则;
  文件名尾部多余段(如 "name - auth - vol2.suffix")忽略; 全部落空时回退为完整文件名(auth 为空)
- 常用预设(配置页一键插入, 可增删改排序): "name - auth" / "auth - name" / "仅 name" / 空白自定义兜底
- 默认预设: split=" - ", name=${arr[0]}, auth=${arr[1]}

### 规则归属
- include_subfixs 与 info_parse_fns **每个仓库各存一份**, 存在该 repo 自己 sqlite 的 meta 表(键同名, JSON 数组)
- config.toml 里的同名两项是**全局默认值**: 仓库未自定义时继承; 保存过自己的规则后各走各的
- 添加仓库时不固化全局规则副本, 保证全局默认对未自定义仓库始终生效
- 旧库兼容: 打开即自动迁移(见下)

### 旧库迁移(打开 db 时自动执行)
- items: 旧 pk 直接变成新自增 id(标签关联不变), 旧字符串 id 复制进 name, auth 置空, 下次扫描按新规则刷新
- 旧 id_gen_fns 规则转换成 info_parse 规则(原 template 变 name 模板, auth 置空)后删旧键; 无法解析则删键, 继承新全局默认
- config.toml 中的旧 id_gen_fns 键被忽略, 下次保存即消失
