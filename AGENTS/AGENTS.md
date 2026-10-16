
## 需求
本地标签管理系统 - closetag: bun + actix-web -> 单二进制

存储位置: 用户目录, 按规范来, 命名: closetag

基本配置信息: repo_paths, include_subfixs(筛选文件后缀), id_gen_fns('-'分割,${arr[0]||arr[1]}之类的)

tags: id(int), name, parent_id, description

items: id(string), path(相对repo), tags

tag_groups: id(int), name, includes(null: all), excludes(), description

一个repo一个db, 合理生成增删改查逻辑和页面, 命名可自行规范; 启动自动打开页面, 关闭页面自动退出