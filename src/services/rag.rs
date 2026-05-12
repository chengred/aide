#![allow(dead_code)]

use std::collections::HashMap;

/// A document chunk in the vector store
#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub language: String,
}

/// A search result from the retrieval engine
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub document: Document,
    pub score: f64,
    pub match_type: MatchType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchType {
    Semantic,
    Keyword,
    Hybrid,
}

/// Simple in-memory vector store with cosine similarity
pub struct VectorStore {
    documents: Vec<Document>,
    embeddings: HashMap<String, Vec<f32>>,
}

impl VectorStore {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            embeddings: HashMap::new(),
        }
    }

    /// Add a document with its embedding
    pub fn add(&mut self, doc: Document, embedding: Vec<f32>) {
        self.embeddings.insert(doc.id.clone(), embedding);
        self.documents.push(doc);
    }

    /// Search by vector similarity
    pub fn search_semantic(&self, query_embedding: &[f32], top_k: usize) -> Vec<(usize, f64)> {
        let mut scores: Vec<(usize, f64)> = self
            .documents
            .iter()
            .enumerate()
            .filter_map(|(idx, doc)| {
                self.embeddings.get(&doc.id).map(|emb| {
                    let score = cosine_similarity(query_embedding, emb);
                    (idx, score)
                })
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

    /// Number of documents
    pub fn len(&self) -> usize {
        self.documents.len()
    }
}

/// BM25 text search engine
pub struct BM25Index {
    documents: Vec<Document>,
    /// Term frequency per document
    term_freqs: Vec<HashMap<String, usize>>,
    /// Document frequency per term
    doc_freqs: HashMap<String, usize>,
    /// Average document length
    avg_dl: f64,
    /// Total number of documents
    n_docs: usize,
    /// BM25 parameters
    k1: f64,
    b: f64,
}

impl BM25Index {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            term_freqs: Vec::new(),
            doc_freqs: HashMap::new(),
            avg_dl: 0.0,
            n_docs: 0,
            k1: 1.5,
            b: 0.75,
        }
    }

    /// Add a document to the index
    pub fn add(&mut self, doc: Document) {
        let terms = tokenize(&doc.content);
        let mut tf = HashMap::new();
        for term in &terms {
            *tf.entry(term.clone()).or_insert(0) += 1;
            *self.doc_freqs.entry(term.clone()).or_insert(0) += 1;
        }
        self.term_freqs.push(tf);
        self.documents.push(doc);
        self.n_docs += 1;

        // Update average document length
        let total_len: f64 = self.term_freqs.iter().map(|t| t.values().sum::<usize>() as f64).sum();
        self.avg_dl = total_len / self.n_docs as f64;
    }

    /// Search with BM25 scoring
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(usize, f64)> {
        let query_terms = tokenize(query);
        let mut scores: Vec<(usize, f64)> = Vec::new();

        for (idx, tf_map) in self.term_freqs.iter().enumerate() {
            let doc_len: usize = tf_map.values().sum();
            let mut score = 0.0;

            for term in &query_terms {
                let tf = *tf_map.get(term).unwrap_or(&0) as f64;
                let df = *self.doc_freqs.get(term).unwrap_or(&0) as f64;

                if tf == 0.0 {
                    continue;
                }

                // BM25 formula
                let idf = ((self.n_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();
                let numerator = tf * (self.k1 + 1.0);
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * doc_len as f64 / self.avg_dl);
                score += idf * numerator / denominator;
            }

            if score > 0.0 {
                scores.push((idx, score));
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }
}

/// The RAG engine: hybrid retrieval combining BM25 + semantic search
pub struct RagEngine {
    vector_store: VectorStore,
    bm25_index: BM25Index,
    /// Weight for semantic vs keyword (0.0 = all keyword, 1.0 = all semantic)
    semantic_weight: f64,
}

impl RagEngine {
    /// Number of indexed documents
    pub fn len(&self) -> usize {
        self.bm25_index.len()
    }

    pub fn new() -> Self {
        Self {
            vector_store: VectorStore::new(),
            bm25_index: BM25Index::new(),
            semantic_weight: 0.5,
        }
    }

    /// Index a code file by splitting it into chunks
    pub fn index_file(&mut self, file_path: &str, content: &str, language: &str) {
        let lines: Vec<&str> = content.lines().collect();
        let chunk_size = 50; // lines per chunk

        for chunk_start in (0..lines.len()).step_by(chunk_size / 2) {
            let chunk_end = (chunk_start + chunk_size).min(lines.len());
            let chunk_content: String = lines[chunk_start..chunk_end].join("\n");

            if chunk_content.trim().is_empty() {
                continue;
            }

            let id = format!("{}:{}", file_path, chunk_start);
            let doc = Document {
                id: id.clone(),
                content: chunk_content.clone(),
                file_path: file_path.to_string(),
                start_line: chunk_start + 1,
                end_line: chunk_end,
                language: language.to_string(),
            };

            // Generate a simple embedding (mean of word vectors from a simple hash embedding)
            let embedding = simple_embed(&chunk_content, 128);

            self.vector_store.add(doc.clone(), embedding);
            self.bm25_index.add(doc);
        }
    }

    /// Search with hybrid retrieval
    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        // BM25 keyword search
        let keyword_results = self.bm25_index.search(query, top_k * 2);

        // Semantic search (if we have embeddings)
        let query_embedding = simple_embed(query, 128);
        let semantic_results = if self.vector_store.len() > 0 {
            self.vector_store.search_semantic(&query_embedding, top_k * 2)
        } else {
            Vec::new()
        };

        // Merge results with weighted scoring
        let mut merged: HashMap<usize, (f64, MatchType)> = HashMap::new();

        // Normalize and add BM25 scores
        let max_bm25 = keyword_results.first().map(|r| r.1).unwrap_or(1.0);
        for (idx, score) in &keyword_results {
            let normalized = score / max_bm25.max(0.001);
            merged.insert(*idx, (normalized * (1.0 - self.semantic_weight), MatchType::Keyword));
        }

        // Normalize and add semantic scores
        let max_sem = semantic_results.first().map(|r| r.1).unwrap_or(1.0);
        for (idx, score) in &semantic_results {
            let normalized = score / max_sem.max(0.001);
            let entry = merged.entry(*idx).or_insert((0.0, MatchType::Semantic));
            entry.0 += normalized * self.semantic_weight;
            if entry.1 == MatchType::Keyword {
                entry.1 = MatchType::Hybrid;
            }
        }

        // Sort and convert to results
        let mut results: Vec<SearchResult> = merged
            .into_iter()
            .filter_map(|(idx, (score, match_type))| {
                if idx < self.bm25_index.documents.len() {
                    Some(SearchResult {
                        document: self.bm25_index.documents[idx].clone(),
                        score,
                        match_type,
                    })
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Format search results as context for the LLM
    pub fn format_context(&self, results: &[SearchResult]) -> String {
        if results.is_empty() {
            return "No relevant code found.".to_string();
        }

        let mut context = String::from("Relevant code from the codebase:\n\n");
        for (i, result) in results.iter().enumerate() {
            context.push_str(&format!(
                "--- Result {} (score: {:.2}, type: {:?}) ---\n",
                i + 1,
                result.score,
                result.match_type
            ));
            context.push_str(&format!(
                "File: {} (lines {}-{})\n",
                result.document.file_path, result.document.start_line, result.document.end_line
            ));
            context.push_str("```");
            context.push_str(&result.document.language);
            context.push('\n');
            context.push_str(&result.document.content);
            context.push_str("\n```\n\n");
        }
        context
    }
}

impl Default for RagEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let terms = tokenize("hello world foo_bar baz123");
        assert!(terms.contains(&"hello".to_string()));
        assert!(terms.contains(&"world".to_string()));
        assert!(terms.contains(&"foo_bar".to_string()));
        assert!(terms.contains(&"baz123".to_string()));
    }

    #[test]
    fn test_tokenize_filters_short_terms() {
        let terms = tokenize("a b c ab cd ef");
        assert!(!terms.contains(&"a".to_string()));
        assert!(!terms.contains(&"b".to_string()));
        assert!(terms.contains(&"ab".to_string()));
    }

    #[test]
    fn test_bm25_add_and_search() {
        let mut index = BM25Index::new();
        index.add(Document {
            id: "1".into(),
            content: "rust programming language with async support".into(),
            file_path: "test.rs".into(),
            start_line: 1,
            end_line: 1,
            language: "rust".into(),
        });
        index.add(Document {
            id: "2".into(),
            content: "python data science machine learning".into(),
            file_path: "test.py".into(),
            start_line: 1,
            end_line: 1,
            language: "python".into(),
        });
        index.add(Document {
            id: "3".into(),
            content: "rust async tokio runtime for concurrent programming".into(),
            file_path: "lib.rs".into(),
            start_line: 1,
            end_line: 1,
            language: "rust".into(),
        });

        assert_eq!(index.len(), 3);

        let results = index.search("rust async", 5);
        assert!(!results.is_empty());
        // Doc 0 or 2 should rank highest for "rust async"
        let top_idx = results[0].0;
        assert!(top_idx == 0 || top_idx == 2);
    }

    #[test]
    fn test_bm25_empty_search() {
        let mut index = BM25Index::new();
        index.add(Document {
            id: "1".into(),
            content: "hello world".into(),
            file_path: "test.txt".into(),
            start_line: 1,
            end_line: 1,
            language: "text".into(),
        });
        let results = index.search("zzzz_nonexistent_xyz", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_simple_embed() {
        let emb = simple_embed("hello world", 128);
        assert_eq!(emb.len(), 128);
        // Should be normalized (unit vector)
        let norm: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01 || norm == 0.0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.01);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_rag_index_and_search() {
        let mut engine = RagEngine::new();
        engine.index_file("main.rs", "fn main() {\n    println!(\"hello\");\n}", "rust");
        engine.index_file("lib.py", "def hello():\n    print('hello')", "python");

        assert_eq!(engine.len(), 2);

        let results = engine.search("rust main function", 3);
        assert!(!results.is_empty());
        assert!(results[0].document.file_path.contains("main.rs"));
    }

    #[test]
    fn test_rag_format_context() {
        let engine = RagEngine::new();
        let results = vec![
            SearchResult {
                document: Document {
                    id: "test.rs:0".into(),
                    content: "fn main() {}".into(),
                    file_path: "test.rs".into(),
                    start_line: 1,
                    end_line: 1,
                    language: "rust".into(),
                },
                score: 0.95,
                match_type: MatchType::Hybrid,
            },
        ];
        let ctx = engine.format_context(&results);
        assert!(ctx.contains("test.rs"));
        assert!(ctx.contains("fn main()"));
    }
}


/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum();
    let norm_a: f64 = a.iter().map(|x| *x as f64 * *x as f64).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| *x as f64 * *x as f64).sum::<f64>().sqrt();

    if norm_a < 1e-8 || norm_b < 1e-8 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Simple tokenizer for BM25
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && s.len() >= 2)
        .map(|s| s.to_lowercase())
        .collect()
}

/// Generate a simple embedding vector using a hash-based approach.
/// For production use, replace with a proper embedding model (BGE-M3, etc.).
fn simple_embed(text: &str, dim: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dim];
    let terms = tokenize(text);

    for term in &terms {
        // Simple hash to position
        let hash = term.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        let pos = (hash % dim as u64) as usize;
        vec[pos] += 1.0;
    }

    // Normalize
    let sum: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if sum > 0.0 {
        for v in &mut vec {
            *v /= sum;
        }
    }

    vec
}
