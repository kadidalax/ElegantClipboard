mod repository;
mod schema;

pub use repository::*;
pub use schema::*;

use crate::clipboard::{compute_semantic_hash, is_url};
use parking_lot::{Mutex, RwLock};
use rusqlite::{Connection, OpenFlags, params};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[derive(Clone)]
pub struct ActiveDatabase {
    pub(crate) write_conn: Arc<Mutex<Connection>>,
    pub(crate) read_conn: Arc<Mutex<Connection>>,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub images_dir: PathBuf,
    pub icons_dir: PathBuf,
    pub staged_dir: PathBuf,
}

/// 稳定数据库句柄；业务库可热切换，全局设置库固定。
pub struct Database {
    pub(crate) active: Arc<RwLock<ActiveDatabase>>,
    settings_conn: Arc<Mutex<Connection>>,
    operation: Arc<RwLock<()>>,
}

impl Database {
    #[cfg(not(test))]
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        Self::new_with_settings(db_path, get_app_dir().join("settings.db"))
    }

    #[cfg(test)]
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        let settings_path = db_path.with_extension("settings.db");
        Self::new_with_settings(db_path, settings_path)
    }

    pub(crate) fn new_with_settings(
        db_path: PathBuf,
        settings_db_path: PathBuf,
    ) -> Result<Self, rusqlite::Error> {
        let active = Self::open_db_path(db_path)?;
        let settings_conn = Connection::open(settings_db_path)?;
        settings_conn.execute_batch("PRAGMA busy_timeout = 5000;")?;

        let db = Self {
            active: Arc::new(RwLock::new(active)),
            settings_conn: Arc::new(Mutex::new(settings_conn)),
            operation: Arc::new(RwLock::new(())),
        };

        db.init_settings_for(&db.active_snapshot())?;

        Ok(db)
    }

    fn open_db_path(db_path: PathBuf) -> Result<ActiveDatabase, rusqlite::Error> {
        let data_dir = db_path
            .parent()
            .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
        std::fs::create_dir_all(&data_dir)
            .map_err(|_| rusqlite::Error::InvalidPath(data_dir.clone()))?;
        let data_dir = std::fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let db_path = data_dir.join(
            db_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("clipboard.db")),
        );
        let write_conn = Connection::open(&db_path)?;
        Self::configure_connection(&write_conn, false)?;
        let read_conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure_connection(&read_conn, true)?;
        let active = ActiveDatabase {
            write_conn: Arc::new(Mutex::new(write_conn)),
            read_conn: Arc::new(Mutex::new(read_conn)),
            images_dir: data_dir.join("images"),
            icons_dir: data_dir.join("icons"),
            staged_dir: data_dir.join("staged"),
            data_dir,
            db_path,
        };
        Self::init_schema_for(&active)?;
        info!("Database opened at {:?}", active.db_path);
        Ok(active)
    }

    pub fn open_active(&self, data_dir: PathBuf) -> Result<ActiveDatabase, rusqlite::Error> {
        let active = Self::open_db_path(data_dir.join("clipboard.db"))?;
        self.init_settings_for(&active)?;
        Ok(active)
    }

    pub fn active_snapshot(&self) -> ActiveDatabase {
        self.active.read().clone()
    }

    pub fn swap_active(&self, target: ActiveDatabase) -> ActiveDatabase {
        std::mem::replace(&mut *self.active.write(), target)
    }

    fn init_settings_for(&self, active: &ActiveDatabase) -> Result<(), rusqlite::Error> {
        let legacy: Vec<(String, String)> = {
            let conn = active.write_conn.lock();
            let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut conn = self.settings_conn.lock();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch(SETTINGS_SCHEMA_SQL)?;
        let migrated = tx.query_row(
            "SELECT COUNT(*) > 0 FROM settings_metadata WHERE key='legacy_settings_migrated'",
            [],
            |r| r.get::<_, bool>(0),
        )?;
        let settings_empty = tx.query_row("SELECT COUNT(*) = 0 FROM settings", [], |r| {
            r.get::<_, bool>(0)
        })?;
        if !migrated && settings_empty {
            for (key, value) in legacy {
                if key.starts_with('_') {
                    continue;
                }
                tx.execute(
                    "INSERT OR IGNORE INTO settings(key,value) VALUES(?1,?2)",
                    params![key, value],
                )?;
            }
        }
        if !migrated {
            tx.execute(
                "INSERT INTO settings_metadata(key,value) VALUES('legacy_settings_migrated','1')",
                [],
            )?;
        }
        tx.execute_batch(DEFAULT_SETTINGS_SQL)?;
        tx.commit()
    }

    fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), rusqlite::Error> {
        if read_only {
            conn.execute_batch(
                "PRAGMA query_only = ON;
                 PRAGMA cache_size = -32000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;",
            )?;
        } else {
            conn.execute_batch(
                "PRAGMA busy_timeout = 5000;
                 PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA foreign_keys = ON;",
            )?;
        }
        Ok(())
    }

    fn init_schema_for(active: &ActiveDatabase) -> Result<(), rusqlite::Error> {
        let conn = active.write_conn.lock();

        Self::run_migrations(&conn)?;

        conn.execute_batch(SCHEMA_SQL)?;
        info!("Database schema initialized");

        Ok(())
    }

    /// 数据库迁移（在 schema 创建前执行）
    fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !table_exists {
            return Ok(());
        }

        Self::recover_orphan_clipboard_items_table(conn)?;

        // 迁移 1: sort_order
        let has_sort_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'sort_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_sort_order {
            info!("Migrating database: adding sort_order column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN sort_order INTEGER DEFAULT 0;
                 UPDATE clipboard_items SET sort_order = id;",
            )?;
            info!("Migration complete: sort_order column added");
        }

        // 迁移 2: 移除 FTS5（改用 LIKE 支持中文搜索）
        let has_fts: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_fts'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if has_fts {
            info!("Migrating database: removing FTS5 table and triggers");
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS clipboard_items_ai;
                 DROP TRIGGER IF EXISTS clipboard_items_ad;
                 DROP TRIGGER IF EXISTS clipboard_items_au;
                 DROP TABLE IF EXISTS clipboard_fts;",
            )?;
            info!("Migration complete: FTS5 removed");
        }

        // 迁移 3: char_count
        let has_char_count: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'char_count'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_char_count {
            info!("Migrating database: adding char_count column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN char_count INTEGER;
                 UPDATE clipboard_items SET char_count = LENGTH(text_content) WHERE text_content IS NOT NULL;"
            )?;
            info!("Migration complete: char_count column added");
        }

        // 迁移 4: image_width/image_height
        let has_image_width: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'image_width'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_image_width {
            info!("Migrating database: adding image_width and image_height columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN image_width INTEGER;
                 ALTER TABLE clipboard_items ADD COLUMN image_height INTEGER;",
            )?;
            info!("Migration complete: image_width and image_height columns added");
        }

        // 迁移 5: source_app_name/source_app_icon
        let has_source_app: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'source_app_name'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_source_app {
            info!("Migrating database: adding source_app_name and source_app_icon columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN source_app_name TEXT;
                 ALTER TABLE clipboard_items ADD COLUMN source_app_icon TEXT;",
            )?;
            info!("Migration complete: source_app columns added");
        }

        // 迁移 6: 添加 group_id 并将 content_hash 唯一性改为分组内唯一（重建表）
        let has_group_id: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'group_id'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_group_id {
            info!("Migrating database: adding group_id column (table rebuild)");

            // 确认 item_groups 表是否存在（用于迁移旧分组关联数据）
            let has_item_groups: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='item_groups'",
                [],
                |row| row.get(0),
            ).unwrap_or(false);

            let tx = conn.unchecked_transaction()?;

            // 先确保 groups 表存在（schema 顺序已调整，但旧库可能没有）
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );",
            )?;

            // 建新表（含 group_id）
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );",
            )?;

            // 复制数据：若 item_groups 存在则从中取 MIN(group_id)，否则全部设为 NULL（默认分组）
            if has_item_groups {
                tx.execute_batch(
                    "INSERT INTO clipboard_items_new 
                     SELECT id, content_type, text_content, html_content, rtf_content,
                            image_path, file_paths, content_hash, content_hash, preview, byte_size,
                            image_width, image_height, is_pinned, is_favorite,
                            CASE WHEN is_favorite = 1 THEN sort_order ELSE 0 END, sort_order,
                            created_at, updated_at, access_count, last_accessed_at, char_count,
                            source_app_name, source_app_icon,
                            (SELECT MIN(ig.group_id) FROM item_groups ig WHERE ig.item_id = clipboard_items.id)
                     FROM clipboard_items;"
                )?;
            } else {
                tx.execute_batch(
                    "INSERT INTO clipboard_items_new 
                     SELECT id, content_type, text_content, html_content, rtf_content,
                            image_path, file_paths, content_hash, content_hash, preview, byte_size,
                            image_width, image_height, is_pinned, is_favorite,
                            CASE WHEN is_favorite = 1 THEN sort_order ELSE 0 END, sort_order,
                            created_at, updated_at, access_count, last_accessed_at, char_count,
                            source_app_name, source_app_icon, NULL
                     FROM clipboard_items;",
                )?;
            }

            // 处理重复 hash（同一分组内可能有多条相同 hash 的记录，保留最新一条）
            tx.execute_batch(
                "DELETE FROM clipboard_items_new WHERE id NOT IN (
                    SELECT MAX(id) FROM clipboard_items_new
                    GROUP BY COALESCE(CAST(group_id AS TEXT), 'NULL'), content_hash
                );",
            )?;

            // 删除旧表并重命名
            tx.execute_batch(
                "DROP TABLE clipboard_items;
                 ALTER TABLE clipboard_items_new RENAME TO clipboard_items;
                 DROP TABLE IF EXISTS item_groups;
                 -- 重建索引
                 CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);
                 -- 重建触发器
                 CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
                 AFTER UPDATE ON clipboard_items
                 BEGIN
                     UPDATE clipboard_items SET updated_at = datetime('now', 'localtime') WHERE id = new.id;
                 END;"
            )?;

            tx.commit()?;
            info!("Migration complete: group_id column added, table rebuilt");
        }

        // 迁移 7: favorite_order
        let has_favorite_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'favorite_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_favorite_order {
            info!("Migrating database: adding favorite_order column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN favorite_order INTEGER DEFAULT 0;
                 UPDATE clipboard_items
                 SET favorite_order = sort_order
                 WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order
                   ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;",
            )?;
            info!("Migration complete: favorite_order column added");
        }

        // 迁移 8: 允许重复 content_hash（always_new 策略），保留索引但移除唯一约束
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_clipboard_hash_default;
             DROP INDEX IF EXISTS idx_clipboard_hash_group;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_default
               ON clipboard_items(content_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_group
               ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;",
        )?;

        // Migration 8: add semantic_hash and backfill existing rows.
        let has_semantic_hash: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'semantic_hash'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_semantic_hash {
            info!("Migrating database: adding semantic_hash column");
            conn.execute_batch("ALTER TABLE clipboard_items ADD COLUMN semantic_hash TEXT;")?;
        }

        Self::backfill_semantic_hashes(conn)?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default
               ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group
               ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;",
        )?;

        // 迁移 9: 修复 issue #81 收藏顺序乱跳的根因——存量数据中存在重复或零值
        // favorite_order，导致收藏列表排序退化为 sort_order/created_at 等次级键，
        // 进而被复制/粘贴动作引发的 bump_to_top / touch_by_column 间接打乱。
        // 这里按用户当前看到的顺序 (favorite_order DESC, id DESC) 重新分配单调递增、
        // 互不相同的整数；toggle_favorite / move_favorite_item_by_id 已天然保证
        // favorite_order 唯一，所以一次规整后就不会再退化。
        Self::normalize_favorite_order(conn)?;

        // 迁移 10: 新增 url 内容类型，并将存量单行链接从 text 归类为 url
        Self::migrate_url_content_type(conn)?;

        // 迁移 11: 文件剪贴板 fidelity payload（CF_HDROP + 伴生格式 + staging）
        let has_file_payload: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'file_payload'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_file_payload {
            info!("Migrating database: adding file_payload column");
            conn.execute_batch("ALTER TABLE clipboard_items ADD COLUMN file_payload TEXT;")?;
            info!("Migration complete: file_payload column added");
        }

        for (name, definition) in [
            ("source_title", "TEXT"),
            ("source_url", "TEXT"),
            ("source_file_name", "TEXT"),
            ("is_locked", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = ?1",
                    [name],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !exists {
                conn.execute_batch(&format!(
                    "ALTER TABLE clipboard_items ADD COLUMN {name} {definition};"
                ))?;
            }
        }

        Ok(())
    }

    /// 若迁移过程中进程异常退出，可能留下 clipboard_items_new 而无 clipboard_items
    fn recover_orphan_clipboard_items_table(conn: &Connection) -> Result<(), rusqlite::Error> {
        let has_new: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items_new'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        let has_old: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if has_new && !has_old {
            tracing::warn!("Recovering orphaned clipboard_items_new from interrupted migration");
            conn.execute_batch("ALTER TABLE clipboard_items_new RENAME TO clipboard_items;")?;
        }
        Ok(())
    }

    fn url_content_type_migration_done(conn: &Connection) -> Result<bool, rusqlite::Error> {
        let settings_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !settings_exist {
            return Ok(false);
        }
        conn.query_row(
            "SELECT COALESCE((SELECT value FROM settings WHERE key = '_migration_url_content_type' LIMIT 1), '') = 'done'",
            [],
            |row| row.get(0),
        )
    }

    fn mark_url_content_type_migration_done(conn: &Connection) -> Result<(), rusqlite::Error> {
        let settings_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if settings_exist {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('_migration_url_content_type', 'done')",
                [],
            )?;
        }
        Ok(())
    }

    fn migrate_url_content_type(conn: &Connection) -> Result<(), rusqlite::Error> {
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok(());
        }

        // 旧库：迁移 10 曾通过重建表把 'url' 写进 CHECK 约束
        let legacy_done: bool = conn
            .query_row(
                "SELECT COALESCE((SELECT sql FROM sqlite_master WHERE type='table' AND name='clipboard_items'), '') LIKE '%''url''%'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if legacy_done || Self::url_content_type_migration_done(conn)? {
            Self::mark_url_content_type_migration_done(conn)?;
            return Ok(());
        }

        info!("Migrating database: adding url content_type");
        conn.execute_batch(
            "UPDATE clipboard_items
             SET semantic_hash = content_hash
             WHERE semantic_hash IS NULL OR semantic_hash = '';",
        )?;

        Self::strip_content_type_check_if_present(conn)?;
        Self::backfill_url_content_types(conn)?;
        Self::mark_url_content_type_migration_done(conn)?;
        info!("Migration complete: url content_type added");
        Ok(())
    }

    /// 旧表若仍带 content_type CHECK，回填 url 前先去约束（重建表）
    fn strip_content_type_check_if_present(conn: &Connection) -> Result<(), rusqlite::Error> {
        let has_check: bool = conn
            .query_row(
                "SELECT COALESCE((SELECT sql FROM sqlite_master WHERE type='table' AND name='clipboard_items'), '') LIKE '%CHECK(content_type%'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_check {
            return Ok(());
        }

        let has_file_payload: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'file_payload'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        info!("Migrating database: removing content_type CHECK constraint");
        let tx = conn.unchecked_transaction()?;
        if has_file_payload {
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    file_payload TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                INSERT INTO clipboard_items_new
                SELECT id, content_type, text_content, html_content, rtf_content,
                       image_path, file_paths, file_payload, content_hash,
                       COALESCE(NULLIF(semantic_hash, ''), content_hash),
                       preview, byte_size, image_width, image_height,
                       is_pinned, is_favorite, favorite_order, sort_order,
                       created_at, updated_at, access_count, last_accessed_at,
                       char_count, source_app_name, source_app_icon, group_id
                FROM clipboard_items;
                DROP TABLE clipboard_items;
                ALTER TABLE clipboard_items_new RENAME TO clipboard_items;",
            )?;
        } else {
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                INSERT INTO clipboard_items_new
                SELECT id, content_type, text_content, html_content, rtf_content,
                       image_path, file_paths, content_hash,
                       COALESCE(NULLIF(semantic_hash, ''), content_hash),
                       preview, byte_size, image_width, image_height,
                       is_pinned, is_favorite, favorite_order, sort_order,
                       created_at, updated_at, access_count, last_accessed_at,
                       char_count, source_app_name, source_app_icon, group_id
                FROM clipboard_items;
                DROP TABLE clipboard_items;
                ALTER TABLE clipboard_items_new RENAME TO clipboard_items;",
            )?;
        }
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);
             CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
             AFTER UPDATE ON clipboard_items
             BEGIN
                 UPDATE clipboard_items SET updated_at = datetime('now', 'localtime')
                 WHERE id = new.id;
             END;",
        )?;
        tx.commit()?;
        Ok(())
    }

    fn backfill_url_content_types(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, text_content FROM clipboard_items WHERE content_type = 'text' AND text_content IS NOT NULL",
        )?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(std::result::Result::ok)
            .collect();

        let mut update =
            conn.prepare_cached("UPDATE clipboard_items SET content_type = 'url', semantic_hash = content_hash WHERE id = ?1")?;
        let mut count = 0;
        for (id, text) in rows {
            if is_url(&text) {
                update.execute(params![id])?;
                count += 1;
            }
        }
        if count > 0 {
            info!("Backfilled {} url content types", count);
        }
        Ok(())
    }

    /// 将所有 `is_favorite = 1` 的条目的 `favorite_order` 规整为
    /// 单调递增、互不相同的整数，保留当前显示顺序。
    /// 当不存在重复或零值时直接跳过，避免无谓写入。
    fn normalize_favorite_order(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 是否存在并列或为零的 favorite_order
        let needs_fix: bool = conn.query_row(
            "SELECT (COUNT(*) > COUNT(DISTINCT favorite_order)) \
             OR EXISTS(SELECT 1 FROM clipboard_items WHERE is_favorite = 1 AND favorite_order <= 0) \
             FROM clipboard_items WHERE is_favorite = 1",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !needs_fix {
            return Ok(());
        }

        // 取出所有收藏项 id，按当前显示顺序排列（最前面的在最前）
        let ids: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM clipboard_items \
                 WHERE is_favorite = 1 \
                 ORDER BY favorite_order DESC, id DESC",
            )?;
            stmt.query_map([], |row| row.get::<_, i64>(0))?
                .filter_map(std::result::Result::ok)
                .collect()
        };

        if ids.is_empty() {
            return Ok(());
        }

        info!(
            "Migrating database: normalizing favorite_order for {} items",
            ids.len()
        );

        let total = ids.len() as i64;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE clipboard_items SET favorite_order = ?1 \
                 WHERE id = ?2 AND is_favorite = 1",
            )?;
            for (idx, id) in ids.iter().enumerate() {
                let new_order = total - idx as i64; // 最前面的得到最大值
                stmt.execute(params![new_order, id])?;
            }
        }
        tx.commit()?;

        info!("Migration complete: favorite_order normalized");
        Ok(())
    }

    fn backfill_semantic_hashes(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 版本守卫：已完成的迁移不再执行，避免每次启动全表扫描
        let settings_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if settings_exist {
            let already_done: bool = conn
                .query_row(
                    "SELECT COALESCE((SELECT value FROM settings WHERE key = '_migration_backfill_semantic_hash' LIMIT 1), '') = 'done'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if already_done {
                return Ok(());
            }
        }

        let mut stmt = conn.prepare(
            "SELECT id, content_type, text_content, content_hash, semantic_hash
             FROM clipboard_items
             WHERE semantic_hash IS NULL
                OR semantic_hash = ''
                OR (content_type IN ('text', 'html', 'rtf') AND semantic_hash = content_hash)",
        )?;

        let mut updates: Vec<(i64, String)> = Vec::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        for row in rows {
            let (id, content_type, text_content, content_hash, existing_semantic_hash) = row?;
            let computed_semantic_hash =
                compute_semantic_hash(&content_type, text_content.as_deref(), &content_hash);

            if existing_semantic_hash.as_deref() != Some(computed_semantic_hash.as_str()) {
                updates.push((id, computed_semantic_hash));
            }
        }
        drop(stmt);

        let tx = conn.unchecked_transaction()?;
        if !updates.is_empty() {
            let updated_count = updates.len();
            {
                let mut update_stmt =
                    tx.prepare("UPDATE clipboard_items SET semantic_hash = ?1 WHERE id = ?2")?;
                for (id, semantic_hash) in updates {
                    update_stmt.execute(params![semantic_hash, id])?;
                }
            }
            info!(
                "Migration complete: semantic_hash backfilled for {} rows",
                updated_count
            );
        }

        // 写入版本标记，后续启动跳过
        if settings_exist {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('_migration_backfill_semantic_hash', 'done')",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn write_connection(&self) -> Arc<Mutex<Connection>> {
        self.active.read().write_conn.clone()
    }

    #[cfg(test)]
    pub fn read_connection(&self) -> Arc<Mutex<Connection>> {
        self.active.read().read_conn.clone()
    }

    pub fn settings_connection(&self) -> Arc<Mutex<Connection>> {
        self.settings_conn.clone()
    }

    pub fn operation_lock(&self) -> Arc<RwLock<()>> {
        self.operation.clone()
    }

    pub fn optimize(&self) -> Result<(), rusqlite::Error> {
        let write_conn = self.write_connection();
        let conn = write_conn.lock();
        conn.execute_batch("PRAGMA optimize;")?;
        info!("Database optimized");
        Ok(())
    }

    pub fn vacuum(&self) -> Result<(), rusqlite::Error> {
        let write_conn = self.write_connection();
        let conn = write_conn.lock();
        conn.execute_batch("VACUUM;")?;
        info!("Database vacuumed");
        Ok(())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            active: self.active.clone(),
            settings_conn: self.settings_conn.clone(),
            operation: self.operation.clone(),
        }
    }
}

/// 获取应用安装目录（可执行文件所在目录）
pub fn get_app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_default_db_path() -> PathBuf {
    get_app_dir().join("clipboard.db")
}

#[cfg(test)]
mod global_settings_tests {
    use super::*;

    #[test]
    fn global_settings_are_isolated_migrated_once_and_shared_by_clones() {
        let dir = std::env::temp_dir().join(format!("elegant-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let business1 = dir.join("one.db");
        let settings = dir.join("settings.db");
        let legacy = Connection::open(&business1).unwrap();
        legacy.execute_batch(SCHEMA_SQL).unwrap();
        legacy
            .execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('theme','dark')",
                [],
            )
            .unwrap();
        drop(legacy);

        let db1 = Database::new_with_settings(business1, settings.clone()).unwrap();
        let repo = SettingsRepository::new(&db1);
        assert_eq!(repo.get("theme").unwrap().as_deref(), Some("dark"));
        repo.set("only_global", "yes").unwrap();
        let clone = db1.clone();
        assert!(Arc::ptr_eq(
            &db1.settings_connection(),
            &clone.settings_connection()
        ));
        assert_eq!(
            Connection::open(&settings)
                .unwrap()
                .query_row("SELECT COUNT(*) FROM clipboard_items", [], |r| r
                    .get::<_, i64>(0))
                .unwrap_err()
                .sqlite_error_code(),
            Some(rusqlite::ErrorCode::Unknown)
        );
        assert_eq!(
            Connection::open(db1.active_snapshot().db_path)
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM settings WHERE key='only_global'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );

        let business2 = dir.join("two.db");
        let legacy = Connection::open(&business2).unwrap();
        legacy.execute_batch(SCHEMA_SQL).unwrap();
        legacy
            .execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('theme','light')",
                [],
            )
            .unwrap();
        drop(legacy);
        let db2 = Database::new_with_settings(business2, settings).unwrap();
        assert_eq!(
            SettingsRepository::new(&db2)
                .get("theme")
                .unwrap()
                .as_deref(),
            Some("dark")
        );
        drop(repo);
        drop(clone);
        drop(db1);
        drop(db2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_initialization_migrates_only_one_business_database() {
        let dir = std::env::temp_dir().join(format!("elegant-db-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.db");
        let paths = [dir.join("one.db"), dir.join("two.db")];
        for (path, source) in paths.iter().zip(["one", "two"]) {
            let conn = Connection::open(path).unwrap();
            conn.execute_batch(SCHEMA_SQL).unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('source',?1)",
                [source],
            )
            .unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('paired',?1)",
                [format!("{source}-pair")],
            )
            .unwrap();
        }
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let settings = settings.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    Database::new_with_settings(path, settings).unwrap()
                })
            })
            .collect();
        let dbs: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let repo = SettingsRepository::new(&dbs[0]);
        let source = repo.get("source").unwrap().unwrap();
        assert_eq!(
            repo.get("paired").unwrap().unwrap(),
            format!("{source}-pair")
        );
        drop(repo);
        drop(dbs);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reset_settings_does_not_revive_legacy_values_on_reopen() {
        let dir = std::env::temp_dir().join(format!("elegant-db-reset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let business = dir.join("business.db");
        let settings = dir.join("settings.db");
        let legacy = Connection::open(&business).unwrap();
        legacy.execute_batch(SCHEMA_SQL).unwrap();
        legacy
            .execute(
                "INSERT INTO settings(key,value) VALUES('legacy_only','old')",
                [],
            )
            .unwrap();
        drop(legacy);
        let db = Database::new_with_settings(business.clone(), settings.clone()).unwrap();
        let repo = SettingsRepository::new(&db);
        assert_eq!(repo.get("legacy_only").unwrap().as_deref(), Some("old"));
        repo.clear_all().unwrap();
        drop(repo);
        drop(db);
        let reopened = Database::new_with_settings(business, settings).unwrap();
        let repo = SettingsRepository::new(&reopened);
        assert_eq!(repo.get("legacy_only").unwrap(), None);
        assert_eq!(repo.get("theme").unwrap().as_deref(), Some("system"));
        drop(repo);
        drop(reopened);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn existing_global_settings_without_marker_are_not_overwritten_by_legacy() {
        let dir = std::env::temp_dir().join(format!("elegant-db-marker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let business = dir.join("business.db");
        let settings = dir.join("settings.db");
        let legacy = Connection::open(&business).unwrap();
        legacy.execute_batch(SCHEMA_SQL).unwrap();
        legacy
            .execute(
                "INSERT INTO settings(key,value) VALUES('legacy_only','old')",
                [],
            )
            .unwrap();
        drop(legacy);
        let global = Connection::open(&settings).unwrap();
        global.execute_batch(SETTINGS_SCHEMA_SQL).unwrap();
        global
            .execute(
                "INSERT INTO settings(key,value) VALUES('custom','keep')",
                [],
            )
            .unwrap();
        drop(global);
        let db = Database::new_with_settings(business, settings).unwrap();
        let repo = SettingsRepository::new(&db);
        assert_eq!(repo.get("custom").unwrap().as_deref(), Some("keep"));
        assert_eq!(repo.get("legacy_only").unwrap(), None);
        assert_eq!(repo.get("theme").unwrap().as_deref(), Some("system"));
        assert_eq!(
            db.settings_connection()
                .lock()
                .query_row(
                    "SELECT COUNT(*) FROM settings_metadata WHERE key='legacy_settings_migrated'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        drop(repo);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> Database {
        let dir = std::env::temp_dir().join(format!("ec_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("test_{}.db", uuid_simple()));
        Database::new(path).unwrap()
    }

    fn uuid_simple() -> u64 {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[test]
    fn new_database_creates_schema() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn new_database_has_source_metadata_columns() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        for column in [
            "source_title",
            "source_url",
            "source_file_name",
            "is_locked",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = ?1",
                    [column],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing column {column}");
        }
    }

    #[test]
    fn migration_adds_source_metadata_columns_to_existing_database() {
        let dir = std::env::temp_dir().join(format!("ec_source_mig_{}", uuid_simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.db");
        let legacy_schema = SCHEMA_SQL
            .replace("    source_title TEXT,\n", "")
            .replace("    source_url TEXT,\n", "")
            .replace("    source_file_name TEXT,\n", "")
            .replace("    is_locked INTEGER NOT NULL DEFAULT 0,\n", "");
        Connection::open(&path)
            .unwrap()
            .execute_batch(&legacy_schema)
            .unwrap();

        let db = Database::new(path).unwrap();
        let conn = db.read_connection();
        let conn = conn.lock();
        for column in [
            "source_title",
            "source_url",
            "source_file_name",
            "is_locked",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = ?1",
                    [column],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing migrated column {column}");
        }
    }

    #[test]
    fn schema_creates_groups_table() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='groups'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn business_settings_stays_empty_and_global_settings_has_defaults() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(conn);
        let settings = db.settings_connection();
        let val: String = settings
            .lock()
            .query_row(
                "SELECT value FROM settings WHERE key = 'global_shortcut'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "Alt+C");
    }

    #[test]
    fn read_write_separation() {
        let db = temp_db();
        {
            let w = db.write_connection();
            let w = w.lock();
            w.execute(
                "INSERT INTO settings (key, value) VALUES ('test_rw', 'hello')",
                [],
            )
            .unwrap();
        }
        let r = db.read_connection();
        let r = r.lock();
        let val: String = r
            .query_row(
                "SELECT value FROM settings WHERE key = 'test_rw'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "hello");
    }

    #[test]
    fn optimize_and_vacuum() {
        let db = temp_db();
        assert!(db.optimize().is_ok());
        assert!(db.vacuum().is_ok());
    }

    #[test]
    fn clone_shares_connections() {
        let db = temp_db();
        let db2 = db.clone();
        {
            let w = db.write_connection();
            let w = w.lock();
            w.execute(
                "INSERT INTO settings (key, value) VALUES ('clone_test', 'shared')",
                [],
            )
            .unwrap();
        }
        let r = db2.read_connection();
        let r = r.lock();
        let val: String = r
            .query_row(
                "SELECT value FROM settings WHERE key = 'clone_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "shared");
    }

    #[test]
    fn migration_10_adds_url_content_type_from_legacy_schema() {
        let dir = std::env::temp_dir().join(format!("ec_mig10_{}", uuid_simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.db");

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );
                CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT);
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash, preview
                ) VALUES
                    ('text', 'https://example.com', 'hash_url', NULL, 'https://example.com'),
                    ('text', 'plain note', 'hash_text', '', 'plain note');",
            )
            .unwrap();
        }

        let db = Database::new(path).unwrap();
        let conn = db.read_connection();
        let conn = conn.lock();

        let url_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(url_count, 1);

        let null_semantic: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE semantic_hash IS NULL OR semantic_hash = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(null_semantic, 0);
    }

    #[test]
    fn migration_6_accepts_legacy_url_content_type_without_group_id() {
        let dir = std::env::temp_dir().join(format!("ec_mig6_{}", uuid_simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.db");

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT
                );
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, preview
                ) VALUES
                    ('url', 'https://example.com', 'hash_url', 'https://example.com'),
                    ('video', 'legacy', 'hash_video', 'legacy');",
            )
            .unwrap();
        }

        let db = Database::new(path).unwrap();
        let conn = db.read_connection();
        let conn = conn.lock();

        let has_group_id: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'group_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_group_id);

        let url_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(url_count, 1);

        let video_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'video'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(video_count, 1);
    }
}

#[cfg(test)]
mod active_database_switch_tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ec-active-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn text_item(text: &str) -> NewClipboardItem {
        let hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some(text.into()),
            content_hash: hash.clone(),
            semantic_hash: hash,
            preview: Some(text.into()),
            ..Default::default()
        }
    }

    #[test]
    fn existing_clipboard_repository_follows_active_database() {
        let root = temp_root("clipboard");
        let first_dir = root.join("first");
        let second_dir = root.join("second");
        let db =
            Database::new_with_settings(first_dir.join("clipboard.db"), root.join("settings.db"))
                .unwrap();
        let repo = ClipboardRepository::new(&db);
        repo.insert(text_item("first")).unwrap();

        let second = db.open_active(second_dir.clone()).unwrap();
        let first = db.swap_active(second);
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 0);
        repo.insert(text_item("second")).unwrap();

        let second = db.swap_active(first);
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 1);
        assert_eq!(
            repo.list(QueryOptions::default()).unwrap()[0]
                .preview
                .as_deref(),
            Some("first")
        );
        db.swap_active(second);
        assert_eq!(
            repo.list(QueryOptions::default()).unwrap()[0]
                .preview
                .as_deref(),
            Some("second")
        );
        drop(repo);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_group_repository_follows_active_database() {
        let root = temp_root("groups");
        let db =
            Database::new_with_settings(root.join("first/clipboard.db"), root.join("settings.db"))
                .unwrap();
        let repo = GroupRepository::new(&db);
        repo.create("first", None).unwrap();

        let second = db.open_active(root.join("second")).unwrap();
        let first = db.swap_active(second);
        assert!(repo.list_with_count().unwrap().is_empty());
        repo.create("second", None).unwrap();

        db.swap_active(first);
        assert_eq!(repo.list_with_count().unwrap()[0].name, "first");
        drop(repo);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_repository_stays_on_global_database() {
        let root = temp_root("settings");
        let db =
            Database::new_with_settings(root.join("first/clipboard.db"), root.join("settings.db"))
                .unwrap();
        let repo = SettingsRepository::new(&db);
        repo.set("switch-test", "same").unwrap();
        let second = db.open_active(root.join("second")).unwrap();
        db.swap_active(second);
        assert_eq!(repo.get("switch-test").unwrap().as_deref(), Some("same"));
        drop(repo);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_resource_snapshot_changes_with_database() {
        let root = temp_root("paths");
        let first_dir = root.join("first");
        let second_dir = root.join("second");
        let db =
            Database::new_with_settings(first_dir.join("clipboard.db"), root.join("settings.db"))
                .unwrap();
        let first_dir = std::fs::canonicalize(first_dir).unwrap();
        assert_eq!(db.active_snapshot().images_dir, first_dir.join("images"));
        assert_eq!(db.active_snapshot().icons_dir, first_dir.join("icons"));
        assert_eq!(db.active_snapshot().staged_dir, first_dir.join("staged"));

        let second = db.open_active(second_dir.clone()).unwrap();
        db.swap_active(second);
        let snapshot = db.active_snapshot();
        assert_eq!(
            snapshot.data_dir,
            std::fs::canonicalize(second_dir).unwrap()
        );
        assert_eq!(snapshot.db_path, snapshot.data_dir.join("clipboard.db"));
        drop(snapshot);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
