#!/usr/bin/env python3
"""
Consolidated Claude.ai conversation extractor
Handles nested lists, thoughts, and responses properly
"""

from bs4 import BeautifulSoup

def extract_text_with_smart_lists(element):
    """Extract text with intelligent nested list structure detection"""
    if not element:
        return ""
    
    parts = []
    children = list(element.children)
    
    i = 0
    while i < len(children):
        child = children[i]
        
        if hasattr(child, 'name'):
            if child.name == 'p':
                text = child.get_text(strip=True)
                if text:
                    parts.append(text)
                    
            elif child.name in ['ol', 'ul']:
                # Check if this is a header list (items ending with ':')
                current_list_items = child.find_all('li')
                
                # Look for pattern: header list followed by content list
                if (current_list_items and 
                    len(current_list_items) == 1 and 
                    current_list_items[0].get_text(strip=True).endswith(':') and
                    i + 1 < len(children) and
                    hasattr(children[i + 1], 'name') and 
                    children[i + 1].name in ['ul', 'ol']):
                    
                    # This is a header followed by content
                    header = current_list_items[0].get_text(strip=True)
                    parts.append(f"**{header}**")
                    
                    # Process the next list as indented content (no extra spacing)
                    i += 1
                    next_child = children[i]
                    content_items = next_child.find_all('li')
                    
                    indented_items = []
                    for li in content_items:
                        item_text = li.get_text(strip=True)
                        if item_text:
                            indented_items.append(f"  - {item_text}")
                    
                    if indented_items:
                        # Join list items without extra spacing
                        parts.append('\n'.join(indented_items))
                else:
                    # Regular list processing
                    list_items = []
                    for li in current_list_items:
                        item_text = li.get_text(strip=True)
                        if item_text:
                            if item_text.endswith(':'):
                                # This is a standalone header
                                list_items.append(f"**{item_text}**")
                            else:
                                list_items.append(f"- {item_text}")
                    
                    if list_items:
                        parts.append('\n'.join(list_items))  # Join with single \n
                                
            elif child.name == 'div':
                nested_text = extract_text_with_smart_lists(child)
                if nested_text:
                    parts.append(nested_text)
        else:
            # Text node
            text = str(child).strip()
            if text:
                parts.append(text)
        
        i += 1
    
    # Join parts with smart spacing - avoid double spacing for nested lists
    result_parts = []
    for i, part in enumerate(parts):
        result_parts.append(part)
        # Add double spacing except between headers and their lists
        if (i < len(parts) - 1 and 
            not (part.startswith('**') and part.endswith(':**') and 
                 parts[i + 1].startswith('  - '))):
            result_parts.append('')
    
    return '\n'.join(result_parts).strip()

def extract_user_messages(soup):
    """Extract all user messages"""
    user_messages = []
    message_divs = soup.find_all('div', {'data-testid': 'user-message'})
    
    for div in message_divs:
        text = extract_text_with_smart_lists(div)
        if text:
            user_messages.append(text)
    
    return user_messages

def extract_claude_responses(soup):
    """Extract Claude thoughts and responses with proper separation"""
    responses = []
    
    streaming_divs = soup.find_all('div', {'data-is-streaming': 'false'})
    
    for container in streaming_divs:
        response_data = {'thoughts': '', 'response': ''}
        
        # First extract actual responses from standard-markdown divs
        response_parts = []
        markdown_divs = container.find_all('div', class_='grid-cols-1 standard-markdown')
        for md_div in markdown_divs:
            response_text = extract_text_with_smart_lists(md_div)
            if response_text and len(response_text) > 10:
                response_parts.append(response_text)
        
        if response_parts:
            response_data['response'] = '\n\n'.join(response_parts)
        
        # Extract thoughts by finding font-claude-response divs that DON'T contain standard-markdown
        thought_divs = container.find_all('div', class_='font-claude-response')
        for thought_div in thought_divs:
            # Skip if this contains standard-markdown (that's the response section)
            if thought_div.find('div', class_='standard-markdown'):
                continue
                
            thought_text = extract_text_with_smart_lists(thought_div)
            if thought_text and len(thought_text) > 50:
                response_data['thoughts'] = thought_text
                break
        
        if response_data['thoughts'] or response_data['response']:
            responses.append(response_data)
    
    return responses

def main():
    # Use the clean HTML file by default
    html_file = '../docs/codetriever-concept-clean.html'
    output_file = '../docs/codetriever-concept.md'
    
    print("🔍 Reading HTML file...")
    with open(html_file, 'r', encoding='utf-8') as f:
        html_content = f.read()
    
    soup = BeautifulSoup(html_content, 'lxml')
    
    print("📝 Extracting user messages...")
    user_messages = extract_user_messages(soup)
    print(f"Found {len(user_messages)} user messages")
    
    print("🤖 Extracting Claude responses...")
    claude_responses = extract_claude_responses(soup)
    print(f"Found {len(claude_responses)} Claude responses")
    
    # Build final markdown
    markdown_content = "# Codetriever Concept - Claude Conversation\n\n"
    markdown_content += "Extracted from Claude.ai conversation with proper nested list formatting\n\n"
    markdown_content += "---\n\n"
    
    max_messages = max(len(user_messages), len(claude_responses))
    
    for i in range(max_messages):
        if i < len(user_messages):
            markdown_content += f"## 👤 User Message {i+1}\n\n"
            markdown_content += f"{user_messages[i]}\n\n"
        
        if i < len(claude_responses):
            markdown_content += f"## 🤖 Claude Response {i+1}\n\n"
            
            if claude_responses[i]['thoughts']:
                markdown_content += "### 🧠 Claude's Thought Process\n\n"
                markdown_content += f"{claude_responses[i]['thoughts']}\n\n"
            
            if claude_responses[i]['response']:
                markdown_content += "### 🚀 Claude's Response\n\n"
                markdown_content += f"{claude_responses[i]['response']}\n\n"
            
            markdown_content += "---\n\n"
    
    with open(output_file, 'w', encoding='utf-8') as f:
        f.write(markdown_content)
    
    print(f"✅ Final extraction saved to {output_file}")
    print(f"📊 {len(user_messages)} user messages, {len(claude_responses)} Claude responses")
    print("🎯 With proper nested list formatting!")

if __name__ == "__main__":
    main()