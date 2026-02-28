use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::Any;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Type-erased cached value trait for internal storage.
trait CachedValue: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn to_yaml(&self) -> Result<String>;
}

/// Concrete typed cache wrapper.
struct TypedCache<T: Serialize + 'static> {
    value: T,
}

impl<T: Serialize + 'static> CachedValue for TypedCache<T> {
    fn as_any(&self) -> &dyn Any {
        &self.value
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.value
    }

    fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(&self.value)?)
    }
}

/// A data node corresponding to a single `.yaml` file on disk.
///
/// Maintains a type-erased in-memory cache. Type parameters are specified
/// on individual method calls (get/set/load) rather than on the node itself.
pub struct DataNode {
    path: PathBuf,
    cache: RefCell<Option<Box<dyn CachedValue>>>,
    dirty: Cell<bool>,
}

impl DataNode {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: RefCell::new(None),
            dirty: Cell::new(false),
        }
    }

    /// Returns the disk path of this node's `.yaml` file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Asynchronously load data from disk into cache.
    ///
    /// If the node is already loaded, this is a no-op.
    /// If the file doesn't exist, creates a default value in cache (not written to disk yet).
    pub async fn load<T>(&self) -> Result<()>
    where
        T: DeserializeOwned + Serialize + Default + 'static,
    {
        // Check if already loaded
        let cache = self.cache.borrow();
        if cache.is_some() {
            return Ok(());
        }
        drop(cache);

        // Load from disk if file exists, otherwise create default
        if !self.path.exists() {
            // File doesn't exist, initialize cache with default value
            let value = T::default();
            *self.cache.borrow_mut() = Some(Box::new(TypedCache { value }));
            return Ok(());
        }

        let contents = tokio::fs::read_to_string(&self.path)
            .await
            .with_context(|| format!("failed to read {}", self.path.display()))?;

        let value: T = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;

        *self.cache.borrow_mut() = Some(Box::new(TypedCache { value }));
        Ok(())
    }

    /// Get a read-only reference to the cached data.
    ///
    /// Returns `None` if the node has not been loaded or set.
    pub fn get<T>(&self) -> Result<Option<Ref<'_, T>>>
    where
        T: 'static,
    {
        let borrow = self.cache.borrow();
        if borrow.is_none() {
            return Ok(None);
        }

        Ok(Some(Ref::map(borrow, |opt| {
            opt.as_ref()
                .unwrap()
                .as_any()
                .downcast_ref::<T>()
                .unwrap()
        })))
    }

    /// Get a mutable reference to the cached data.
    ///
    /// Marks the node as dirty. Call `flush()` to persist changes.
    /// Returns `None` if the node has not been loaded or set.
    pub fn get_mut<T>(&self) -> Result<Option<RefMut<'_, T>>>
    where
        T: 'static,
    {
        let borrow = self.cache.borrow_mut();
        if borrow.is_none() {
            return Ok(None);
        }

        self.dirty.set(true);
        Ok(Some(RefMut::map(borrow, |opt| {
            opt.as_mut()
                .unwrap()
                .as_any_mut()
                .downcast_mut::<T>()
                .unwrap()
        })))
    }

    /// Set the node's value.
    ///
    /// Marks the node as dirty. Call `flush()` to persist to disk.
    pub fn set<T>(&self, value: T) -> Result<()>
    where
        T: Serialize + 'static,
    {
        *self.cache.borrow_mut() = Some(Box::new(TypedCache { value }));
        self.dirty.set(true);
        Ok(())
    }

    /// Get a mutable reference to the data, creating it with `Default` if not present.
    ///
    /// Marks the node as dirty since mutable access implies potential modification.
    pub fn get_or_default<T>(&self) -> Result<RefMut<'_, T>>
    where
        T: DeserializeOwned + Serialize + Default + 'static,
    {
        if self.cache.borrow().is_none() {
            *self.cache.borrow_mut() = Some(Box::new(TypedCache {
                value: T::default(),
            }));
        }

        // Always mark as dirty when returning mutable reference
        // Since the caller has mutable access, we assume they will modify it
        self.dirty.set(true);

        let borrow = self.cache.borrow_mut();
        Ok(RefMut::map(borrow, |opt| {
            opt.as_mut()
                .unwrap()
                .as_any_mut()
                .downcast_mut::<T>()
                .unwrap()
        }))
    }

    /// Asynchronously persist the cached data to disk if dirty.
    pub async fn flush(&self) -> Result<()> {
        if !self.dirty.get() {
            return Ok(());
        }

        let cache = self.cache.borrow();
        let Some(ref cached) = *cache else {
            return Ok(());
        };

        let yaml_str = cached.to_yaml()?;

        // Ensure parent directories exist
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create dir {}", parent.display()))?;
        }

        tokio::fs::write(&self.path, yaml_str)
            .await
            .with_context(|| format!("failed to write {}", self.path.display()))?;

        self.dirty.set(false);
        Ok(())
    }

    /// Remove the data file from disk and clear the cache.
    pub async fn remove(&self) -> Result<()> {
        if self.path.exists() {
            tokio::fs::remove_file(&self.path)
                .await
                .with_context(|| format!("failed to remove {}", self.path.display()))?;
        }
        *self.cache.borrow_mut() = None;
        self.dirty.set(false);
        Ok(())
    }

    /// Check whether the node has data (in cache or on disk).
    pub fn exists(&self) -> bool {
        self.cache.borrow().is_some() || self.path.exists()
    }

    /// Check whether the node has uncommitted changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
    }
}

/// A directory node in the data store tree.
///
/// Provides hierarchical organization of data nodes.
pub struct DirNode {
    store: Rc<DataStore>,
    base_path: String,
    nodes: RefCell<HashMap<String, Rc<DataNode>>>,
}

impl DirNode {
    /// Get a data node by key (without type parameter).
    ///
    /// The key is relative to this directory. Nodes are cached and reused.
    pub fn node(&self, key: &str) -> Rc<DataNode> {
        let mut nodes = self.nodes.borrow_mut();

        if let Some(node) = nodes.get(key) {
            return Rc::clone(node);
        }

        // Create new node
        let path = if self.base_path.is_empty() {
            format!("{}.yaml", key)
        } else {
            format!("{}/{}.yaml", self.base_path, key)
        };
        let node = Rc::new(DataNode::new(self.store.root.join(path)));
        nodes.insert(key.to_string(), Rc::clone(&node));
        node
    }

    /// Get a subdirectory node.
    ///
    /// The name is relative to this directory.
    pub fn subdir(&self, name: &str) -> Rc<DirNode> {
        Rc::new(DirNode {
            store: Rc::clone(&self.store),
            base_path: if self.base_path.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", self.base_path, name)
            },
            nodes: RefCell::new(HashMap::new()),
        })
    }

    /// Returns the base path of this directory (relative path).
    pub fn path(&self) -> &str {
        &self.base_path
    }

    /// Returns the full filesystem path of this directory.
    pub fn full_path(&self) -> PathBuf {
        self.store.root.join(&self.base_path)
    }
}

/// A tree-structured data store backed by YAML files on disk.
///
/// Each logical node path maps to a `.yaml` file with type information.
pub struct DataStore {
    root: PathBuf,
}

impl DataStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the root directory of this store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the root directory node.
    pub fn root_dir(self: &Rc<Self>) -> Rc<DirNode> {
        Rc::new(DirNode {
            store: Rc::clone(self),
            base_path: String::new(),
            nodes: RefCell::new(HashMap::new()),
        })
    }

    /// List child YAML files under a directory path.
    pub async fn children(&self, dir: &str) -> Result<Vec<String>> {
        let scan_dir = if dir.is_empty() {
            self.root.clone()
        } else {
            self.root.join(dir)
        };

        if !tokio::fs::try_exists(&scan_dir).await? {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        let mut entries = tokio::fs::read_dir(&scan_dir)
            .await
            .with_context(|| format!("failed to read dir {}", scan_dir.display()))?;

        while let Some(entry) = entries.next_entry().await? {
            let file_path = entry.path();
            if file_path.is_file() {
                if let Some(ext) = file_path.extension() {
                    if ext == "yaml" {
                        if let Some(stem) = file_path.file_stem() {
                            names.push(stem.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }

        names.sort();
        Ok(names)
    }

    /// List subdirectories under a directory path.
    pub async fn subdirs(&self, dir: &str) -> Result<Vec<String>> {
        let scan_dir = if dir.is_empty() {
            self.root.clone()
        } else {
            self.root.join(dir)
        };

        if !tokio::fs::try_exists(&scan_dir).await? {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        let mut entries = tokio::fs::read_dir(&scan_dir)
            .await
            .with_context(|| format!("failed to read dir {}", scan_dir.display()))?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_dir() {
                if let Some(name) = entry.path().file_name() {
                    names.push(name.to_string_lossy().into_owned());
                }
            }
        }

        names.sort();
        Ok(names)
    }
}
