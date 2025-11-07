use std::path::{Path, PathBuf};
use tokio::fs;
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use std::collections::{HashMap, HashSet};
use tantivy::{
    schema::{Schema, STORED, TEXT, UNSIGNED_DATE},
    doc, Index, Document,
    collector::TopDocs,
    query::QueryParser,
    DateTime as TantivyDateTime,
};
use chrono::Utc;
use async_trait::async_trait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndex {
    base_path: PathBuf,
    index_path: PathBuf,
    schema: Schema,
    index: Index,
    indexed_files: HashSet<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: PathBuf,
    pub score: f32,
    pub highlights: Vec<String>,
    pub metadata: FileMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub content_types: Option<Vec<ContentType>>,
    pub date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    pub size_range: Option<(u64, u64)>,
    pub tags: Option<Vec<String>>,
    pub max_results: usize,
}

impl FileIndex {
    pub async fn new(base_path: PathBuf) -> Result<Self> {
        let index_path = base_path.join(".skylock").join("index");
        fs::create_dir_all(&index_path).await?;

        let mut schema_builder = Schema::builder();

        // Define schema fields
        schema_builder.add_text_field("path", TEXT | STORED);
        schema_builder.add_text_field("name", TEXT | STORED);
        schema_builder.add_text_field("extension", TEXT | STORED);
        schema_builder.add_unsigned_long_field("size", STORED);
        schema_builder.add_date_field("created", STORED);
        schema_builder.add_date_field("modified", STORED);
        schema_builder.add_text_field("content_type", TEXT | STORED);
        schema_builder.add_text_field("mime_type", TEXT | STORED);
        schema_builder.add_text_field("tags", TEXT | STORED);
        schema_builder.add_text_field("hash", STORED);

        let schema = schema_builder.build();
        let index = Index::create_in_dir(&index_path, schema.clone())?;

        Ok(Self {
            base_path,
            index_path,
            schema,
            index,
            indexed_files: HashSet::new(),
        })
    }

    pub async fn index_file(&mut self, file_info: &FileInfo) -> Result<()> {
        let mut writer = self.index.writer(50_000_000)?; // 50MB buffer

        let path_field = self.schema.get_field("path")?;
        let name_field = self.schema.get_field("name")?;
        let extension_field = self.schema.get_field("extension")?;
        let size_field = self.schema.get_field("size")?;
        let created_field = self.schema.get_field("created")?;
        let modified_field = self.schema.get_field("modified")?;
        let content_type_field = self.schema.get_field("content_type")?;
        let mime_type_field = self.schema.get_field("mime_type")?;
        let tags_field = self.schema.get_field("tags")?;
        let hash_field = self.schema.get_field("hash")?;

        let mut doc = Document::new();
        doc.add_text(path_field, file_info.path.to_string_lossy().as_ref());
        doc.add_text(name_field, &file_info.name);

        if let Some(ref ext) = file_info.extension {
            doc.add_text(extension_field, ext);
        }

        doc.add_u64(size_field, file_info.size);
        doc.add_date(created_field, TantivyDateTime::from_utc(file_info.created));
        doc.add_date(modified_field, TantivyDateTime::from_utc(file_info.modified));
        doc.add_text(content_type_field, format!("{:?}", file_info.metadata.content_type));
        doc.add_text(mime_type_field, &file_info.mime_type);

        for tag in &file_info.tags {
            doc.add_text(tags_field, tag);
        }

        if let Some(ref hash) = file_info.hash {
            doc.add_text(hash_field, hash);
        }

        writer.add_document(doc)?;
        writer.commit()?;

        self.indexed_files.insert(file_info.path.clone());
        Ok(())
    }

    pub async fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.schema.get_field("path")?,
                self.schema.get_field("name")?,
                self.schema.get_field("tags")?,
            ],
        );

        let query = query_parser.parse_query(query)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(options.max_results))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;

            // Apply filters
            if !self.matches_filters(&doc, options) {
                continue;
            }

            let path = self.get_doc_path(&doc)?;
            let metadata = self.get_doc_metadata(&doc)?;

            results.push(SearchResult {
                path,
                score: _score,
                highlights: self.get_highlights(&doc, query.as_ref())?,
                metadata,
            });
        }

        Ok(results)
    }

    fn matches_filters(&self, doc: &Document, options: &SearchOptions) -> bool {
        // Check content type filter
        if let Some(ref content_types) = options.content_types {
            let doc_type = self.get_doc_content_type(doc)
                .unwrap_or(ContentType::Unknown);
            if !content_types.contains(&doc_type) {
                return false;
            }
        }

        // Check date range filter
        if let Some((start, end)) = options.date_range {
            let modified = self.get_doc_date(doc, "modified")
                .unwrap_or_else(|_| Utc::now());
            if modified < start || modified > end {
                return false;
            }
        }

        // Check size range filter
        if let Some((min_size, max_size)) = options.size_range {
            let size = self.get_doc_size(doc).unwrap_or(0);
            if size < min_size || size > max_size {
                return false;
            }
        }

        // Check tags filter
        if let Some(ref required_tags) = options.tags {
            let doc_tags = self.get_doc_tags(doc);
            if !required_tags.iter().all(|tag| doc_tags.contains(tag)) {
                return false;
            }
        }

        true
    }

    fn get_doc_path(&self, doc: &Document) -> Result<PathBuf> {
        let path_field = self.schema.get_field("path")?;
        let path_str = doc.get_first(path_field)
            .and_then(|f| f.as_text())
            .ok_or_else(|| SyncErrorType::MissingField("path".to_string()).into())?;
        Ok(PathBuf::from(path_str))
    }

    fn get_doc_content_type(&self, doc: &Document) -> Result<ContentType> {
        let type_field = self.schema.get_field("content_type")?;
        let type_str = doc.get_first(type_field)
            .and_then(|f| f.as_text())
            .ok_or_else(|| SyncErrorType::MissingField("content_type".to_string()).into())?;
        Ok(serde_json::from_str(type_str)?)
    }

    fn get_doc_size(&self, doc: &Document) -> Result<u64> {
        let size_field = self.schema.get_field("size")?;
        Ok(doc.get_first(size_field)
            .and_then(|f| f.as_u64())
            .unwrap_or(0))
    }

    fn get_doc_date(&self, doc: &Document, field_name: &str) -> Result<DateTime<Utc>> {
        let date_field = self.schema.get_field(field_name)?;
        let date = doc.get_first(date_field)
            .and_then(|f| f.as_date())
            .ok_or_else(|| SyncErrorType::MissingField(field_name.to_string()).into())?;
        Ok(date.into_utc())
    }

    fn get_doc_tags(&self, doc: &Document) -> HashSet<String> {
        let tags_field = self.schema.get_field("tags").unwrap();
        doc.get_all(tags_field)
            .filter_map(|f| f.as_text())
            .map(String::from)
            .collect()
    }

    fn get_doc_metadata(&self, doc: &Document) -> Result<FileMetadata> {
        // Extract metadata fields from document
        let content_type = self.get_doc_content_type(doc)?;

        Ok(FileMetadata {
            is_hidden: false, // These would need to be stored in additional fields
            is_system: false,
            attributes: HashMap::new(),
            content_type,
        })
    }

    fn get_highlights(&self, doc: &Document, query: &dyn tantivy::query::Query) -> Result<Vec<String>> {
        let mut highlights = Vec::new();
        let snippet_generator = self.index.tokenizer_for_field(self.schema.get_field("path")?)?;

        // Generate highlights for relevant fields
        for field_name in &["path", "name", "tags"] {
            let field = self.schema.get_field(field_name)?;
            if let Some(text) = doc.get_first(field).and_then(|f| f.as_text()) {
                let snippet = snippet_generator.snippet(text, query);
                if !snippet.is_empty() {
                    highlights.push(snippet);
                }
            }
        }

        Ok(highlights)
    }

    pub async fn remove_file(&mut self, path: &Path) -> Result<()> {
        let mut writer = self.index.writer(50_000_000)?;
        let path_field = self.schema.get_field("path")?;
        let term = tantivy::Term::from_field_text(path_field, &path.to_string_lossy());
        writer.delete_term(term);
        writer.commit()?;
        self.indexed_files.remove(path);
        Ok(())
    }

    pub async fn update_file(&mut self, file_info: &FileInfo) -> Result<()> {
        self.remove_file(&file_info.path).await?;
        self.index_file(file_info).await?;
        Ok(())
    }

    pub async fn clear_index(&mut self) -> Result<()> {
        let mut writer = self.index.writer(50_000_000)?;
        writer.delete_all_documents()?;
        writer.commit()?;
        self.indexed_files.clear();
        Ok(())
    }

    pub fn is_indexed(&self, path: &Path) -> bool {
        self.indexed_files.contains(path)
    }
}
