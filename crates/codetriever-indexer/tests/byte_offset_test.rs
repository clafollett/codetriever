//! Test that byte offsets are calculated correctly

use codetriever_indexer::parsing::CodeParser;

#[test]
fn test_byte_offsets_with_tree_sitter() {
    let parser = CodeParser::new(None, false, 1000, 100);

    let rust_code = r#"fn first() {
    println!("First function");
}

fn second() {
    println!("Second function");
}"#;

    let chunks = parser.parse(rust_code, "rust", "test.rs").unwrap();

    // Should have 2 chunks for 2 functions
    assert_eq!(chunks.len(), 2);

    // First function should start at byte 0
    let first_chunk = &chunks[0];
    assert_eq!(first_chunk.byte_start, 0);
    assert!(first_chunk.byte_end > 0);
    assert!(first_chunk.content.contains("first"));

    // Second function should start after first
    let second_chunk = &chunks[1];
    assert!(second_chunk.byte_start > first_chunk.byte_end);
    assert!(second_chunk.byte_end > second_chunk.byte_start);
    assert!(second_chunk.content.contains("second"));

    // Byte ranges should be within the original code length
    assert!(second_chunk.byte_end <= rust_code.len());
}

#[test]
fn test_byte_offsets_without_tree_sitter() {
    let parser = CodeParser::new(None, false, 1000, 100);

    // Use a simple text file (no tree-sitter)
    let text = "Line 1\nLine 2\nLine 3";

    let chunks = parser.parse(text, "txt", "test.txt").unwrap();

    // For non-tree-sitter files, we use 0 and content.len()
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.byte_start, 0);
    assert_eq!(chunk.byte_end, text.len());
}
