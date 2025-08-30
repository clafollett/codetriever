#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import re
# Use relative import since we're in the same utils directory
from .config import get_text_length_limit

# Compile regex patterns at module level for efficiency
TECH_TERM_PATTERNS = {
    # File extensions and formats
    r'\bjson\b': 'jay-sawn',
    r'\bxml\b': 'X M L',
    r'\bhtml\b': 'H T M L',
    r'\bcss\b': 'C S S',
    r'\bjs\b': 'J S',
    r'\bpy\b': 'python',
    r'\bsql\b': 'sequel',
    r'\byaml\b': 'yam-el',
    r'\bcsv\b': 'C S V',
    r'\bpdf\b': 'P D F',
    
    # Common acronyms
    r'\bdb\b': 'D B',
    r'\bapi\b': 'A P I',
    r'\burl\b': 'U R L',
    r'\bid\b': 'I D',
    r'\buuid\b': 'U U I D',
    r'\bjwt\b': 'J W T',
    r'\boauth\b': 'Oh auth',
    r'\bhttp\b': 'H T T P',
    r'\bhttps\b': 'H T T P S',
    r'\brest\b': 'rest',
    r'\bcrud\b': 'crud',
    r'\bcli\b': 'C L I',
    r'\bgui\b': 'gooey',
    r'\btts\b': 'text to speech',
    r'\bai\b': 'A I',
    r'\bml\b': 'M L',
    r'\bllm\b': 'L L M',
    r'\btodo\b': 'to do',
    r'\btodos\b': "to do's",
    r'\bwip\b': 'work in progress',
    
    # Programming terms
    r'\benv\b': 'environment',
    
    # File extensions with dots
    r'\.json\b': ' dot jay-sawn',
    r'\.py\b': ' dot python',
    r'\.js\b': ' dot java script',
    r'\.sql\b': ' dot sequel',
    r'\.yml\b': ' dot yam-el',
    r'\.yaml\b': ' dot yam-el',
    
    # Common chat terms and reactions
    r'\bha\b': 'hah',
    r'\bhaha\b': 'hahah',
    r'\blol\b': 'L O L',
    r'\blmao\b': 'lamow',
    r'\brofl\b': 'roffle',
    r'\bhmm\b': 'hmmm',
    r'\bugh\b': 'ugh',
    r'\bmeh\b': 'meh',
    r'\bomg\b': 'oh em gee',
    r'\bwtf\b': 'what the eff',
    r'\bidk\b': "I don't know",
    r'\bimo\b': 'in my opinion',
    r'\bimho\b': 'in my humble opinion',
    r'\bbtw\b': 'by the way',
    r'\bfyi\b': 'eff why eye',
    r'\btbh\b': 'to be honest',
    r'\bsmh\b': 'shaking my head',
    r'\birl\b': 'in real life',
    r'\bdm\b': 'direct message',
    r'\bok\b': 'okay',
    r'\bthx\b': 'thanks',
    r'\bplz\b': 'please',
    r'\byw\b': "you're welcome",
    
    # MAOS project specific
    r'\bmaos\b': 'may-oss'
}

# Pre-compile all patterns for efficiency
COMPILED_PATTERNS = [(re.compile(pattern, re.IGNORECASE), replacement) 
                     for pattern, replacement in TECH_TERM_PATTERNS.items()]

def preserve_inline_code_content(text):
    """
    Intelligently handle inline code - preserve the content for speech while removing wrapper tokens.
    
    Examples:
    - "the `except` block" -> "the except block"  
    - "use `get_config()` function" -> "use get_config() function"
    - "set `timeout=30`" -> "set timeout=30"
    
    This preserves important technical terms that should be spoken naturally.
    """
    def extract_inline_content(match):
        content = match.group(1)
        
        # If it's a single word (likely a keyword), preserve it
        if re.match(r'^\w+$', content):
            return content
            
        # If it's a simple function call or variable, preserve it
        if re.match(r'^[\w_]+\(\)$', content) or re.match(r'^[\w_]+=[\w_]+$', content):
            return content
            
        # If it's complex code (multiple operators, etc.), remove it
        if any(char in content for char in [';', '{', '}', '[', ']', '&&', '||']):
            return ' '
            
        # Default: preserve simple technical terms
        return content
    
    # Apply intelligent inline code handling
    return re.sub(r'`([^`]+)`', extract_inline_content, text)

def clean_text_for_speech(text):
    """
    Clean text to make it suitable for TTS with natural speech markup.
    Remove code blocks, excessive formatting, and add speech pauses/emphasis.
    """
    # Remove code blocks (```...```) entirely
    text = re.sub(r'```[\s\S]*?```', '', text)
    
    # Intelligently handle inline code - preserve content, strip wrapper tokens
    text = preserve_inline_code_content(text)
    
    # Remove markdown links [text](url)
    text = re.sub(r'\[([^\]]+)\]\([^\)]+\)', r'\1', text)
    
    # Remove markdown headers (#, ##, ###)
    text = re.sub(r'^#{1,6}\s+', '', text, flags=re.MULTILINE)
    
    # Convert bullet points and list markers to natural speech with pauses
    text = re.sub(r'^\s*[-*+]\s+', '', text, flags=re.MULTILINE)
    text = re.sub(r'^\s*\d+\.\s+', '', text, flags=re.MULTILINE)
    
    # Remove tool call indicators and XML-like tags
    text = re.sub(r'<[^>]+>', '', text)
    
    # Remove all emojis for better speech
    text = remove_emojis(text)
    
    # Convert technical terms to speech-friendly versions
    text = convert_technical_terms_to_speech(text)
    
    # Add natural speech markup before cleaning whitespace
    text = add_speech_markup(text)
    
    # Convert newlines to pauses (for list items and natural breaks)
    text = re.sub(r'\n', ' ... ', text)
    
    # Remove excessive whitespace but preserve our added pauses
    text = re.sub(r'\s+', ' ', text)
    
    # Clean up multiple consecutive pauses
    text = re.sub(r'(\.\.\.\s*){2,}', '... ', text)
    
    # Clean up and limit length
    text = text.strip()
    
    # Limit to configured TTS length with fallback
    try:
        text_limit = get_text_length_limit()
    except Exception:
        text_limit = 2000  # Fallback if config unavailable
        
    if len(text) > text_limit:
        text = text[:text_limit] + "..."
    
    return text

def add_speech_markup(text):
    """
    Add cross-platform speech markup for natural pauses and emphasis.
    Uses simple punctuation that works across TTS engines.
    """
    # Add pause after exclamation marks and question marks
    text = re.sub(r'([!?])\s+', r'\1... ', text)
    
    # Add slight pause after periods (but not abbreviations)
    text = re.sub(r'(\w)\.(\s+[A-Z])', r'\1... \2', text)
    
    # Add pause after colons
    text = re.sub(r':\s+', ': ... ', text)
    
    # Add emphasis for text in **bold** (convert to caps with pauses)
    text = re.sub(r'\*\*([^*]+)\*\*', r'... \1 ...', text)
    
    # Add emphasis for text in *italics* (add slight pauses)
    text = re.sub(r'\*([^*]+)\*', r'... \1', text)
    
    # Add pause after parenthetical statements
    text = re.sub(r'\)(\s+)', r') ... \1', text)
    
    # Add pause before parenthetical statements
    text = re.sub(r'(\s+)\(', r'\1... (', text)
    
    # Convert dashes to natural pauses
    text = re.sub(r'\s+--?\s+', ' ... ', text)
    
    return text

def handle_elongated_expressions(text):
    """
    Handle elongated expressions to prevent TTS from spelling them out letter by letter.
    Detects repeated letters in any case and ensures natural pronunciation.
    
    Examples:
    - "YESSSSS!" -> "Yesssss!"
    - "yeeesssss" -> "yeeesssss" (already lowercase, no change needed)
    - "NOOOOO" -> "Nooooo"
    - "YEEEssss" -> "Yeeessss"
    - "But API stays API" (only affects words with 3+ repeated letters)
    """
    def fix_elongated_word(match):
        word = match.group(0)
        
        # Check if word has 3+ consecutive repeated letters
        has_repetition = False
        for i in range(len(word) - 2):
            if word[i].lower() == word[i+1].lower() == word[i+2].lower():
                has_repetition = True
                break
        
        if has_repetition:
            # If all uppercase, convert to title case to prevent letter-by-letter reading
            if word.isupper():
                return word.capitalize()
            # If mixed case with uppercase repetition, lowercase it
            elif any(c.isupper() for c in word):
                return word.lower()
        
        return word
    
    # Match any word (including mixed case)
    pattern = re.compile(r'\b\w+\b')
    return pattern.sub(fix_elongated_word, text)

def remove_emojis(text):
    """
    Remove all Unicode emoji ranges from text for clean TTS speech.
    This regex matches most Unicode emoji ranges.
    """
    # Remove various Unicode emoji ranges
    text = re.sub(r'[\U0001F600-\U0001F64F]', '', text)  # emoticons
    text = re.sub(r'[\U0001F300-\U0001F5FF]', '', text)  # symbols & pictographs
    text = re.sub(r'[\U0001F680-\U0001F6FF]', '', text)  # transport & map
    text = re.sub(r'[\U0001F1E0-\U0001F1FF]', '', text)  # flags
    text = re.sub(r'[\U00002600-\U000027BF]', '', text)  # misc symbols
    text = re.sub(r'[\U0001F900-\U0001F9FF]', '', text)  # supplemental symbols
    
    return text

def convert_technical_terms_to_speech(text):
    """
    Convert technical terms and acronyms in prose to natural speech pronunciation.
    Works on full sentences, not just isolated terms.
    
    Examples:
    - "The JSON config file" -> "The jay-sawn configuration file"
    - "Use the API_KEY" -> "Use the A P I KEY" 
    - "Check config.json" -> "Check configuration dot jay-sawn"
    - "Query the SQL database" -> "Query the sequel database"
    - "The user_id field" -> "The user I D field"
    - "api_key variable" -> "A P I key variable"
    - "YESSSSS!" -> "YESSSS!" (not "Y E S S S S")
    - "NOOOOO" -> "NOOOO" (not "N O O O O")
    """
    # Handle elongated expressions (YESSSSS, NOOOOO, etc) before other processing
    # Convert to lowercase for natural speech while preserving repetition
    text = handle_elongated_expressions(text)
    
    # First, replace underscores and hyphens with spaces to break up compound terms
    # This ensures "api_key" becomes "api key" before we process acronyms
    text = text.replace('_', ' ')
    text = text.replace('-', ' ')
    
    # Apply all pre-compiled patterns efficiently
    for pattern, replacement in COMPILED_PATTERNS:
        text = pattern.sub(replacement, text)
    
    return text