#!/usr/bin/env python3
"""
HTML Sanitizer for Claude.ai exported conversations
Strips styling noise and keeps only essential structure
"""

from bs4 import BeautifulSoup
import re

def sanitize_html(input_file, output_file):
    """Clean HTML by removing styling attributes and simplifying structure"""
    
    print("🧹 Reading and parsing HTML...")
    with open(input_file, 'r', encoding='utf-8') as f:
        html_content = f.read()
    
    soup = BeautifulSoup(html_content, 'lxml')
    
    print("🔧 Sanitizing HTML structure...")
    
    # Remove script tags entirely
    for script in soup.find_all('script'):
        script.decompose()
    
    # Remove style tags
    for style in soup.find_all('style'):
        style.decompose()
    
    # Process all elements
    for element in soup.find_all():
        if element.name:
            # Keep only essential attributes
            attrs_to_keep = {}
            
            # Keep data- attributes as they're semantic
            for attr, value in element.attrs.items():
                if attr.startswith('data-'):
                    attrs_to_keep[attr] = value
                elif attr == 'class':
                    # Simplify class names - keep only key ones
                    classes = value if isinstance(value, list) else [value]
                    important_classes = []
                    for cls in classes:
                        if any(keyword in cls for keyword in [
                            'font-claude-response',
                            'standard-markdown', 
                            'progressive-markdown',
                            'grid-cols-1',
                            'user-message'
                        ]):
                            important_classes.append(cls)
                    if important_classes:
                        attrs_to_keep['class'] = important_classes
            
            # Replace all attributes with cleaned ones
            element.attrs = attrs_to_keep
    
    # Write sanitized HTML
    print("💾 Writing sanitized HTML...")
    sanitized_html = str(soup.prettify())
    
    with open(output_file, 'w', encoding='utf-8') as f:
        f.write(sanitized_html)
    
    print(f"✅ Sanitized HTML saved to {output_file}")
    return output_file

def main():
    input_file = '../docs/codetriever-concept.html'
    output_file = '../docs/codetriever-concept-clean.html'
    
    sanitize_html(input_file, output_file)

if __name__ == "__main__":
    main()