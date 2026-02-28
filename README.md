# 大博弈

大博弈 — 以 **1356 年**（元末）为背景的大型战略游戏，风格参考 EU4/Victoria，使用 Rust 构建。

- **后端**：actix-web + RocksDB
- **前端**：Bevy 0.15（2D）
- **通信**：WebSocket（JSON 指令 / bincode 快照）
- **地图数据**：EU5toGIS GeoPackage 数据集（全球 28,573 个省份）
- **界面语言**：中文优先（rust-i18n，简体中文字体）

## 前置条件

- Rust 工具链（stable，2021 edition 及以上）
- `clang` / `clang++`（RocksDB 依赖）
- `mold` 链接器（已配置在 `.cargo/config.toml`）
- EU5toGIS 数据集 — 参见 [Paradox 论坛帖子](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/#post-31141035)
  - `datasets/` 目录，包含 `locations.gpkg`、`ports.gpkg` 和 `rivers.tif`
  - 同版本中的 `06_pops_totals.txt`（省份人口数据）
- EU5 **文本格式**存档（`.eu5`，以 `SAV` 开头，**非** ZIP 压缩包），用于提取省份归属与商品产出

## 初始化步骤

### 1. 复制人口数据

```bash
cp /path/to/EU5toGIS/06_pops_totals.txt assets/pops.tsv
```

### 2. 从存档提取数据

```bash
cargo run -p parse_save -- /path/to/your.eu5 assets/ownership.tsv assets/vassals.tsv assets/merchandize.tsv assets/country_colors.tsv
```

读取存档中的 `compatibility.locations`（省份名称列表）、`locations.locations`（省份→所有者映射）、`dependency`（宗主-附庸关系）、各国 `last_month_produced`（商品产出）以及各国 `color=rgb`（国家颜色），分别写入：

- `assets/ownership.tsv` — 省份标签 → 所有者国家标签
- `assets/vassals.tsv` — 附庸标签 → 宗主标签
- `assets/merchandize.tsv` — 国家标签 + 商品 + 产量
- `assets/country_colors.tsv` — 国家标签 → RGB 颜色（0–255）

### 3. 生成地图资产

```bash
cargo run --release -p mapgen
```

读取 EU5toGIS `datasets/` 中的 `locations.gpkg` 和 `ports.gpkg`，对省份多边形进行三角剖分，输出：

- `assets/map.bin` — 可游玩省份几何体 + 元数据（约 80 MB）
- `assets/terrain.bin` — 不可游玩地形/水域多边形，用于背景渲染（约 40 MB）

建议使用 `--release` 编译，Debug 模式速度较慢。

### 4. 启动服务端

```bash
cargo run -p server
```

监听 `ws://127.0.0.1:8080/ws`。首次运行时加载 `assets/map.bin`、`assets/ownership.tsv`、`assets/vassals.tsv`、`assets/merchandize.tsv`、`assets/pops.tsv`、`assets/province_names.tsv` 和 `assets/country_names.tsv` 生成世界状态，然后持久化至 `daboyi.db/`（RocksDB）。后续运行直接从数据库加载。

### 5. 启动客户端

```bash
cargo run -p client
```

连接服务端并渲染地图。请在服务端运行的同时，在另一个终端启动客户端。

## 游戏流程

1. **开始界面** — 点击"开始游戏"进入国家选择
2. **选择国家** — 点击地图上任意省份，底部栏显示所属国家；点击"以此国开始游戏"进入正式游玩
3. **正式游玩** — 可切换地图模式、暂停/继续模拟、查看省份详情

## 操作说明

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

## 项目结构

```
daboyi/
├── shared/          # 公共类型（GameState、Province、Pop、Good、MapData）
├── server/          # actix-web 服务端、RocksDB 持久化、游戏模拟
├── client/          # Bevy 前端、WebSocket 客户端、地图与地形渲染
├── assets/          # 生成/数据资产（见下表）
├── locales/         # rust-i18n 本地化文件（zh.yml、en.yml）
├── doc/             # 设计与技术文档（中文）
└── tools/
    ├── mapgen/      # GeoPackage → assets/map.bin + assets/terrain.bin
    └── parse_save/  # EU5 文本存档 → TSV 数据文件
```

### `assets/` 目录说明

| 文件 | 来源 | 用途 |
|------|------|------|
| `map.bin` | `cargo run -p mapgen` | 可游玩省份几何体（约 80 MB） |
| `terrain.bin` | `cargo run -p mapgen` | 不可游玩地形多边形（约 40 MB） |
| `rivers.png` | EU5toGIS `datasets/rivers.tif` | 河流叠加贴图 |
| `ownership.tsv` | `cargo run -p parse_save` | 省份标签 → 所有者国家标签 |
| `vassals.tsv` | `cargo run -p parse_save` | 附庸标签 → 宗主标签 |
| `merchandize.tsv` | `cargo run -p parse_save` | 国家标签 + 商品产出（上月） |
| `country_colors.tsv` | `cargo run -p parse_save` | 国家标签 → RGB 颜色（政治地图用） |
| `pops.tsv` | EU5toGIS `06_pops_totals.txt` | 省份人口数量 |
| `province_names.tsv` | qwen 批量翻译 | 省份标签 → 中文名称 |
| `country_names.tsv` | qwen 批量翻译 | 国家标签 → 中文名称 |
| `fonts/NotoSansCJKsc-Regular.otf` | 系统字体 | 简体中文渲染 |
