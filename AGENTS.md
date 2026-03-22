# Daboyi（大博弈）项目指南

## 项目概述

**Daboyi** 是一款以 Rust 编写的大型战略游戏，背景设定在 1356 年（元末），风格参考 EU4/Victoria。项目采用客户端-服务器架构，服务端负责游戏逻辑仿真与持久化，客户端负责地图渲染与用户交互。

### 技术栈

| 层级 | 技术选型 | 说明 |
|------|----------|------|
| 服务端框架 | actix-web 4 | 异步 HTTP/WebSocket 服务器 |
| 持久化 | RocksDB 0.24 | 嵌入式键值数据库 |
| 客户端引擎 | Bevy 0.15 | Rust 原生 ECS 游戏引擎，2D 渲染 |
| 共享类型 | shared crate | 客户端与服务端共用的数据结构 |
| 地图编译 | mapgen 工具 | GeoPackage (GPKG) → bincode 离线处理 |
| 序列化 | serde + bincode/JSON | 命令用 JSON，状态快照用 bincode |
| 国际化 | rust-i18n 3 | 中文优先 UI，YAML 语言文件 |
| 字体 | NotoSansCJKsc-Regular.otf | 简体中文渲染 |
| 编译工具链 | clang + mold | clang 编译 RocksDB，mold 加速链接 |

### 核心设计原则

1. **服务端权威**：所有游戏逻辑在服务端执行，客户端是无状态的，每 tick 接收完整的 `GameState` 快照
2. **客户端驱动的 tick 模型**：客户端按 `GameSpeed` 计时器频率向服务端发送 `Tick` 消息
3. **非对称序列化**：客户端→服务端用 JSON（可读性），服务端→客户端用 bincode（性能）
4. **经济节流**：经济计算每 100 tick 执行一次，日期每 tick 推进一天，存档每 300 tick 写入 RocksDB
5. **安全性**：代码库中无 `unsafe` 块，无 `as` 转换

## 项目结构

```
daboyi/
├── Cargo.toml              # 工作区根配置
├── .cargo/config.toml      # 编译器与链接器配置（mold + clang）
├── shared/                 # 共享类型库
│   └── src/
│       ├── lib.rs          # 核心游戏类型（Country, Province, Pop, GameState 等）
│       ├── map.rs          # 地图几何类型（MapData, MapProvince, TerrainData）
│       └── conv.rs         # 数值转换辅助函数（消除 as 转换）
├── server/                 # 服务端
│   └── src/
│       ├── main.rs         # 入口：HTTP 服务器启动
│       ├── ws.rs           # WebSocket 消息处理
│       ├── db.rs           # RocksDB 持久化层
│       └── game/           # 仿真逻辑
│           ├── mod.rs      # GameSimulation trait
│           ├── data.rs     # 世界生成
│           ├── load.rs     # TSV 资产加载
│           ├── production.rs # 生产系统
│           └── population.rs # 人口消费与增长
├── client/                 # 客户端
│   └── src/
│       ├── main.rs         # 入口：Bevy 应用初始化
│       ├── state.rs        # AppState 枚举（StartScreen/CountrySelection/Playing）
│       ├── menu.rs         # MenuPlugin：开始界面、国家选择
│       ├── net.rs          # WebSocket 网络层
│       ├── capitals.rs     # CapitalsPlugin：首都标记
│       ├── terrain.rs      # TerrainPlugin：地形/水域网格
│       ├── armies.rs       # ArmiesPlugin：军队渲染
│       ├── map/            # MapPlugin：省份网格渲染、交互
│       │   ├── mod.rs      # 插件注册、系统调度
│       │   ├── color.rs    # 着色辅助函数
│       │   └── interact.rs # 相机控制、省份点击
│       └── ui/
│           └── mod.rs      # UiPlugin：HUD、省份信息面板
├── tools/
│   ├── mapgen/             # 地图编译工具
│   │   └── src/main.rs     # GPKG → map.bin + terrain.bin
│   └── parse_save/         # EU5 存档解析工具
│       └── src/main.rs     # .eu5 → TSV 数据文件
├── locales/
│   ├── zh.yml              # 简体中文 UI 字符串（主语言）
│   └── en.yml              # 英文 UI 字符串（备用）
├── assets/                 # 游戏资产（详见下方）
├── doc/                    # 设计与技术文档（中文）
└── daboyi.db/              # RocksDB 数据库目录（运行时生成）
```

## 构建和运行

### 前置条件

- Rust 工具链（stable，2021 edition 及以上）
- `clang` / `clang++`（RocksDB 依赖）
- `mold` 链接器（已配置在 `.cargo/config.toml`）
- EU5toGIS 数据集（参见 Paradox 论坛）
- EU5 文本格式存档（`.eu5`，以 `SAV` 开头）

### 初始化步骤

#### 1. 复制人口数据

```bash
cp /path/to/EU5toGIS/06_pops_totals.txt assets/pops.tsv
```

#### 2. 从存档提取数据

```bash
cargo run -p parse_save -- /path/to/your.eu5 assets/ownership.tsv assets/vassals.tsv assets/merchandize.tsv assets/country_colors.tsv
```

这将生成以下文件：
- `assets/ownership.tsv` — 省份标签 → 所有者国家标签
- `assets/vassals.tsv` — 附庸标签 → 宗主标签
- `assets/merchandize.tsv` — 国家标签 + 商品 + 产量
- `assets/country_colors.tsv` — 国家标签 → RGB 颜色

#### 3. 生成地图资产

```bash
cargo run --release -p mapgen
```

将生成：
- `assets/map.bin` — 可游玩省份几何体（约 80 MB）
- `assets/terrain.bin` — 不可游玩地形/水域多边形（约 40 MB）

**注意**：建议使用 `--release` 编译，Debug 模式速度较慢。

#### 4. 启动服务端

```bash
cargo run -p server
```

服务端将监听 `ws://127.0.0.1:8080/ws`，首次运行时加载资产并生成世界状态，后续运行直接从 `daboyi.db/` 加载。

#### 5. 启动客户端

```bash
cargo run -p client
```

在另一个终端启动客户端，连接服务端并渲染地图。

### 常用命令

| 命令 | 说明 |
|------|------|
| `cargo build` | 构建整个工作区 |
| `cargo build -p server` | 仅构建服务端 |
| `cargo build -p client` | 仅构建客户端 |
| `cargo build -p mapgen` | 构建地图生成工具 |
| `cargo build -p parse_save` | 构建存档解析工具 |
| `cargo test` | 运行所有测试 |
| `cargo clippy` | 运行 Clippy 检查 |
| `cargo fmt` | 格式化代码 |

## 游戏流程

### 状态机

```
StartScreen ──(点击"开始游戏")──→ CountrySelection ──(点击"以此国开始")──→ Playing
     ↑                                    │
     └──────────(点击"返回")──────────────┘
```

### 各状态说明

| 状态 | 说明 |
|------|------|
| **StartScreen** | 全屏标题覆盖层，政治地图在背景渲染但不可交互 |
| **CountrySelection** | 政治地图；点击省份显示所属国家；可点击"以此国开始" |
| **Playing** | 正式游玩；HUD、省份面板、首都标记、地图模式切换、Tick 发送均激活 |

### 操作说明

| 输入 | 操作 |
|------|------|
| 右键拖拽 | 平移视角 |
| 滚轮 | 缩放 |
| 左键单击 | 选择省份 |
| `1` | 省份模式（EU5 识别颜色） |
| `2` | 人口模式（热力图） |
| `3` | 产出模式（热力图） |
| `4` | 地形模式 |
| `5` | 政治模式（国家归属） |
| 空格 | 暂停 / 继续模拟 |

## 核心数据类型

### GameState（服务器权威）

```rust
pub struct GameState {
    pub tick: u64,
    pub date: GameDate,              // 游戏日期（从 1356 年开始）
    pub countries: Vec<Country>,     // 所有国家
    pub provinces: Vec<Province>,    // 所有省份（~21,111 个）
    pub building_types: Vec<BuildingType>,
    pub vassals: HashMap<String, String>,  // 附庸 → 宗主
    pub armies: Vec<Army>,           // 所有军队
}
```

### Country（国家）

```rust
pub struct Country {
    pub tag: String,                        // 3 字母标签，如 "MNG"、"YUA"
    pub name: String,                       // 中文显示名称
    pub capital_province: u32,              // 首都省份 ID
    pub produced_goods: Vec<(String, f32)>, // 上月产出商品
    pub treasury: f32,                      // 国库资金
}
```

### Province（省份）

```rust
pub struct Province {
    pub id: u32,
    pub name: String,
    pub owner: Option<String>,              // 所有者国家标签
    pub pops: Vec<Pop>,                     // 人口列表
    pub buildings: Vec<Building>,           // 建筑列表
    pub stockpile: GoodsBundle,             // 商品库存
}
```

### Pop（人口）

```rust
pub struct Pop {
    pub class: PopClass,                    // 人口阶层（佃农、自耕农、地主等）
    pub size: u32,                          // 人口数量
    pub needs_satisfaction: f32,            // 需求满足度（0.0-1.0）
}
```

### 消息协议

#### 客户端 → 服务端（JSON）

```rust
pub enum ClientMsg {
    Tick,                    // 推进模拟一 tick
    FetchState,              // 获取当前状态快照
    IssueOrder(Order),       // 执行玩家命令
    SetPlayerCountry(String), // 设置玩家控制的国家
}
```

#### 服务端 → 客户端（bincode）

```rust
pub enum ServerMsg {
    StateSnapshot(GameState),  // 完整游戏状态快照
    Ack,                       // 命令确认
}
```

## 开发约定

### Rust 编码规则

1. **禁止 `as` 转换**：所有数值转换必须使用 `shared/src/conv.rs` 中的命名辅助函数
   - 例如：`f64_to_f32`、`u32_to_usize`、`usize_to_u32`
   - 如果需要新的转换函数，请在 `conv.rs` 中添加，使用 `From`/`TryFrom`/`.try_into().unwrap()`

2. **禁止 `unsafe` 代码**：所有代码必须使用安全的 Rust
   - 无 `unsafe fn`
   - 无 `unsafe impl`
   - 无 `unsafe {}` 块

### 文件操作

- **禁止使用 `rm` 删除文件**：始终使用以下命令删除文件：
  ```bash
  kioclient move "file://$the_file_to_delete" 'trash:/'
  ```

### 测试约定

- 每个模块应包含单元测试
- 集成测试放在 `tests/` 目录
- 使用 `cargo test` 运行所有测试

### 提交规范

- 提交消息使用英文
- 遵循 Conventional Commits 规范：
  - `feat:` 新功能
  - `fix:` 修复 bug
  - `refactor:` 重构
  - `docs:` 文档更新
  - `test:` 测试相关
  - `chore:` 构建/工具相关

## 资产文件说明

### `assets/` 目录

| 文件 | 来源 | 用途 |
|------|------|------|
| `map.bin` | `cargo run -p mapgen` | 可游玩省份几何体（约 80 MB，gitignored） |
| `terrain.bin` | `cargo run -p mapgen` | 不可游玩地形多边形（约 40 MB，gitignored） |
| `rivers.png` | EU5toGIS `datasets/rivers.tif` | 河流叠加贴图 |
| `ownership.tsv` | `cargo run -p parse_save` | 省份标签 → 所有者国家标签 |
| `vassals.tsv` | `cargo run -p parse_save` | 附庸标签 → 宗主标签 |
| `merchandize.tsv` | `cargo run -p parse_save` | 国家标签 + 商品产出 |
| `country_colors.tsv` | `cargo run -p parse_save` | 国家标签 → RGB 颜色 |
| `pops.tsv` | EU5toGIS `06_pops_totals.txt` | 省份人口数量 |
| `province_names.tsv` | qwen 批量翻译 | 省份标签 → 中文名称 |
| `country_names.tsv` | qwen 批量翻译 | 国家标签 → 中文名称 |
| `fonts/NotoSansCJKsc-Regular.otf` | 系统字体 | 简体中文渲染 |

### 工作区依赖

工作区级别的依赖（在 `Cargo.toml` 中定义）：

```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
futures-util = "0.3"
```

各子 crate 应使用这些共享依赖版本。

## 文档资源

项目包含详细的技术文档（中文），位于 `doc/` 目录：

- `架构总览.md` - 整体架构和技术栈说明
- `地图系统.md` - 地图渲染和交互系统
- `网络与通信.md` - WebSocket 通信协议
- `持久化.md` - RocksDB 数据持久化
- `生产系统.md` - 经济生产系统
- `人口系统.md` - 人口消费与增长
- `国际化.md` - rust-i18n 国际化实现
- `数据提取.md` - 从 EU5 存档提取数据

## 常见任务

### 添加新的玩家命令

1. 在 `shared/src/lib.rs` 的 `OrderKind` 枚举中添加新变体
2. 在 `server/src/game/mod.rs` 的 `GameSimulation` trait 中实现处理逻辑
3. 在客户端添加 UI 按钮或键盘快捷键触发该命令

### 修改游戏平衡性参数

- 经济参数在 `server/src/game/params.rs` 中
- 人口参数在 `server/src/game/population.rs` 中
- 生产参数在 `server/src/game/production.rs` 中

### 添加新的地图模式

1. 在 `client/src/map/color.rs` 中添加着色函数
2. 在 `client/src/map/interact.rs` 中添加快捷键绑定
3. 在 `client/src/ui/mod.rs` 中添加 UI 按钮

### 添加新的建筑类型

1. 在 `server/src/game/params.rs` 中定义建筑类型
2. 在 `server/src/game/data.rs` 中初始化建筑
3. 在 `shared/src/lib.rs` 中添加 `OrderKind` 变体（如果需要玩家建造）

## 调试建议

### 服务端调试

- 使用 `RUST_LOG=debug cargo run -p server` 启用调试日志
- 检查 `daboyi.db/` 目录中的数据库状态

### 客户端调试

- 使用 `RUST_LOG=debug cargo run -p client` 启用调试日志
- 使用 Bevy 内置的 FPS 计数器监控性能

### 性能优化

- 使用 `cargo build --release` 进行性能构建
- 使用 `perf` 或 `cargo flamegraph` 进行性能分析
- 关键路径已在 `profile.dev.package."*"` 中设置为 `opt-level = 3`

## 扩展阅读

- [Bevy 官方文档](https://bevyengine.org/learn/book/)
- [actix-web 官方文档](https://docs.rs/actix-web/)
- [RocksDB Rust 绑定文档](https://docs.rs/rocksdb/)
- [EU5toGIS 论坛帖子](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/)

## 联系方式

项目地址：https://github.com/xiaoshihou514/daboyi

---

**最后更新**：2026-03-13