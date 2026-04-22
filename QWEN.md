# Daboyi（大博弈）项目上下文

## 项目概述

**Daboyi** 是一款使用 Rust 和 Bevy 构建的**替代历史地图编辑器/查看器**。项目支持在 EU5 省份地图上绘制自定义国家与行政区划，并将着色结果保存为 JSON 文件。

### 核心特性

- 渲染约 22,688 个全球省份的 2D 地图
- 支持省份着色、国家归属显示
- 首都标记、边界线渲染
- 多种地图模式（省份/地形/政治）
- 中文界面支持

### 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 游戏引擎 | Bevy 0.15 | Rust 原生 ECS，2D 渲染 |
| 语言 | Rust stable (2021 edition) | |
| 编译工具链 | clang + mold | mold 加速链接，clang 编译 C 依赖 |
| 序列化 | serde + JSON/bincode | 着色文件用 JSON，地图几何用 bincode |
| 字体 | NotoSansCJKsc-Regular.otf | 简体中文渲染 |
| 地图数据 | EU5toGIS GeoPackage | 约 22,688 个全球省份 |

---

## 工作区结构

```
daboyi/
├── Cargo.toml              # 工作区根配置
├── .cargo/config.toml      # 编译器与链接器配置（mold + clang）
├── shared/                 # 共享类型库
│   └── src/
│       ├── lib.rs          # 导出模块
│       └── map.rs          # 地图几何类型（MapData, MapProvince, TerrainData 等）
├── client/                 # Bevy 编辑器应用（唯一可运行的二进制）
│   └── src/
│       ├── main.rs         # 入口：Bevy App 初始化
│       ├── state.rs        # AppState 枚举（Loading/Playing）
│       ├── terrain.rs      # TerrainPlugin：荒地/水域渲染
│       ├── capitals.rs     # CapitalsPlugin：首都星形标记
│       ├── map/
│       │   ├── mod.rs      # MapPlugin：省份网格渲染、着色逻辑
│       │   ├── borders.rs  # BordersPlugin：省份边界线渲染
│       │   ├── color.rs    # 着色辅助函数
│       │   └── interact.rs # 相机平移/缩放、省份选择与画笔涂色
│       └── ui/
│           └── mod.rs      # UiPlugin：界面系统
├── assets/                 # 地图与数据资产
├── doc/                    # 技术文档（中文）
└── tools/
    ├── mapgen/             # GeoPackage → map.bin + terrain.bin
    └── parse_save/         # EU5 文本存档 → TSV 数据文件
```

---

## 构建和运行

### 前置条件

- Rust 工具链 (stable, 2021 edition+)
- `clang` / `clang++` (依赖项编译)
- `mold` 链接器
- [EU5toGIS 数据集](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/)

### 资产生成

#### 1. 生成地图几何体

```bash
cargo run --release -p mapgen
```

生成文件:
- `assets/map.bin` — 可游玩省份几何体 (~80 MB)
- `assets/terrain.bin` — 荒地/水域背景 (~40 MB)

**注意**: 必须使用 `--release`，Debug 模式非常慢。

#### 2. (可选) 复制数据文件

```bash
cp /path/to/EU5toGIS/06_pops_totals.txt assets/pops.tsv
```

### 运行查看器

```bash
cargo run -p client
```

---

## 常用命令

| 命令 | 说明 |
|------|------|
| `cargo build` | 构建整个工作区 |
| `cargo build -p client` | 仅构建查看器 |
| `cargo build -p mapgen` | 构建地图生成工具 |
| `cargo build -p parse_save` | 构建存档解析工具 |
| `cargo test` | 运行所有测试 |
| `cargo clippy` | Clippy 检查 |
| `cargo fmt` | 格式化代码 |

---

## 资产文件参考

| 文件 | 来源 | 用途 |
|------|------|------|
| `map.bin` | `cargo run -p mapgen` | 省份几何体 (~80 MB, gitignored) |
| `terrain.bin` | `cargo run -p mapgen` | 荒地/水域几何体 (~40 MB) |
| `rivers.png` | EU5toGIS `datasets/rivers.tif` | 河流叠加贴图 |
| `rivers.bin` | mapgen 生成 | 河流二进制数据 |
| `ownership.tsv` | EU5toGIS / parse_save | 省份标签 → 所有者国家标签 |
| `vassals.tsv` | EU5toGIS / parse_save | 附庸标签 → 宗主标签 |
| `merchandize.tsv` | EU5toGIS / parse_save | 国家标签 + 商品产出 |
| `country_colors.tsv` | EU5toGIS / parse_save | 国家标签 → RGB 颜色 |
| `pops.tsv` | EU5toGIS `06_pops_totals.txt` | 省份人口数量 |
| `province_names.tsv` | 翻译数据 | 省份标签 → 中文名称 |
| `country_names.tsv` | 翻译数据 | 国家标签 → 中文名称 |
| `capitals.tsv` | 翻译数据 | 首都信息 |
| `fonts/NotoSansCJKsc-Regular.otf` | 系统字体 | 中文渲染 |

---

## 核心数据类型

### MapData (shared/src/map.rs)

```rust
pub struct MapData {
    pub provinces: Vec<MapProvince>,
}

pub struct MapProvince {
    pub id: u32,                    // 省份 ID
    pub tag: String,                // EU5 标签，如 "stockholm"
    pub name: String,               // 显示名称
    pub topography: String,         // 地形类型
    pub vegetation: String,         // 植被类型
    pub climate: String,            // 气候类型
    pub raw_material: String,       // 原材料
    pub harbor_suitability: f32,    // 港口适宜度
    pub hex_color: [f32; 4],        // RGBA 颜色
    pub port_sea_zone: Option<String>,
    pub boundary: Vec<Vec<[f32; 2]>>, // 边界多边形
    pub vertices: Vec<[f32; 2]>,    // 三角化顶点
    pub indices: Vec<u32>,          // 三角化索引
    pub centroid: [f32; 2],         // 质心（标签放置）
}
```

### TerrainData

```rust
pub struct TerrainData {
    pub polygons: Vec<TerrainPolygon>,
}

pub struct TerrainPolygon {
    pub color: [f32; 4],
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}
```

---

## 插件架构

```
Bevy App
├── MapPlugin       — 省份网格渲染（单一合并 Mesh2d）、着色模式切换
│   └── BordersPlugin — 相邻省份边界线渲染
├── TerrainPlugin   — 荒地与水域背景网格
├── CapitalsPlugin  — 首都星形标记
└── UiPlugin        — 界面系统
```

### 核心资源 (Resources)

| 资源 | 类型 | 说明 |
|------|------|------|
| `MapResource` | `MapData` | 加载的地图几何数据 |
| `CountryColors` | `HashMap<String, [f32; 4]>` | 国家标签 → RGBA 颜色 |
| `ProvinceNames` | `HashMap<String, String>` | 省份标签 → 中文名称 |
| `Ownership` | `HashMap<u32, String>` | 省份 ID → 国家标签 |
| `SelectedProvince` | `Option<u32>` | 当前选中的省份 ID |
| `MapMode` | 枚举 | 当前地图显示模式 |

### MapMode 枚举

```rust
pub enum MapMode {
    Province,   // 省份模式（EU5 识别颜色）
    Terrain,    // 地形模式
    Political,  // 政治模式（国家归属）
}
```

---

## 开发约定

### Rust 编码规则

1. **允许基础类型使用 `as` 转换**：`u32`/`usize`/`f32`/`f64` 等基础数值类型可直接使用 `as`
   - 涉及范围限制或归一化时，在调用点显式写出 `clamp`/`round` 等逻辑

2. **禁止 `unsafe` 代码**：所有代码必须使用安全的 Rust
   - 无 `unsafe fn`
   - 无 `unsafe impl`
   - 无 `unsafe {}` 块

### 文件操作

- **禁止使用 `rm` 删除文件**：始终使用以下命令删除文件：
  ```bash
  kioclient move "file://$the_file_to_delete" 'trash:/'
  ```

### 提交规范

- 提交消息使用英文
- 遵循 Conventional Commits 规范：
  - `feat:` 新功能
  - `fix:` 修复 bug
  - `refactor:` 重构
  - `docs:` 文档更新
  - `test:` 测试相关
  - `chore:` 构建/工具相关

---

## 编译配置

`.cargo/config.toml`:

```toml
[env]
CC = "clang"
CXX = "clang++"

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

- **mold**：超高速链接器，大幅缩短增量编译时间
- **clang**：编译 C/C++ 依赖库

---

## 调试建议

### 启用调试日志

```bash
RUST_LOG=debug cargo run -p client
```

### 性能优化

- 使用 `cargo build --release` 进行性能构建
- 关键路径已在 `profile.dev.package."*"` 中设置为 `opt-level = 3`
- 使用 `perf` 或 `cargo flamegraph` 进行性能分析

---

## 扩展阅读

- [Bevy 官方文档](https://bevyengine.org/learn/book/)
- [EU5toGIS 论坛帖子](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/)
- `doc/` 目录中的技术文档（中文）

---

## 项目地址

https://github.com/xiaoshihou514/daboyi

---

**最后更新**: 2026-03-26
