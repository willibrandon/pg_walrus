#!/usr/bin/env python3
"""
Constitution Enforcement Hook for Claude Code.

This Stop hook intercepts Claude's responses and BLOCKS any that contain
deferral language. Exit code 2 forces Claude to retry.

Deferral patterns include:
- Hedging: "you might want to", "consider adding"
- Future promises: "can be added later", "phase 2"
- Scope deferral: "out of scope", "beyond scope"
- Responsibility shifting: "you'll need to", "don't forget to"
- Minimization: "basic implementation", "for now"
- Code markers: TODO, FIXME, PLACEHOLDER, HACK, XXX, STUB
"""

import json
import sys
import re

def main():
    # Read hook input from stdin
    try:
        data = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        print(f"Error parsing input: {e}", file=sys.stderr)
        sys.exit(1)

    # Check if we're already in a retry loop to prevent infinite loops
    stop_hook_active = data.get('stop_hook_active', False)
    if stop_hook_active:
        # Already retrying - allow to prevent infinite loop
        sys.exit(0)

    # Get the transcript path
    transcript_path = data.get('transcript_path', '')
    if not transcript_path:
        sys.exit(0)

    # Read the transcript to get Claude's last response
    try:
        with open(transcript_path, 'r') as f:
            transcript_lines = f.readlines()
    except Exception:
        sys.exit(0)

    # Find the last assistant message
    last_response = ''
    for line in reversed(transcript_lines):
        try:
            entry = json.loads(line)
            if entry.get('type') == 'assistant':
                content = entry.get('message', {}).get('content', [])
                if isinstance(content, list):
                    for block in content:
                        if block.get('type') == 'text':
                            last_response += block.get('text', '') + '\n'
                break
        except Exception:
            continue

    if not last_response:
        sys.exit(0)

    # DEFERRAL PATTERNS - PROHIBITED
    deferral_patterns = [
        # Hedging language
        (r'\byou might want to\b', 'Hedging: "you might want to"'),
        (r'\byou could also\b', 'Hedging: "you could also"'),
        (r'\bconsider\s+(adding|implementing|using)\b', 'Hedging: "consider"'),
        (r'\bit would be good to\b', 'Hedging: "it would be good to"'),

        # Future promises
        (r'\bphase 2\b', 'Deferral: "phase 2"'),
        (r'\bfuture enhancement\b', 'Deferral: "future enhancement"'),
        (r'\bfuture iteration\b', 'Deferral: "future iteration"'),
        (r'\bcan be\s+(added|implemented|made)\s+(later|more dynamic)\b', 'Deferral: "can be X later"'),
        (r'\bwe can\s+\w+\s+later\b', 'Deferral: "we can X later"'),
        (r'\bif needed\s*(in the future)?\b', 'Deferral: "if needed"'),
        (r'\bwhen needed\b', 'Deferral: "when needed"'),

        # Scope deferral
        (r'\bout of scope\b', 'Deferral: "out of scope"'),
        (r'\bbeyond\s+(the\s+)?scope\b', 'Deferral: "beyond scope"'),
        (r'\bnot in scope\b', 'Deferral: "not in scope"'),
        (r'\boutside\s+(the\s+)?scope\b', 'Deferral: "outside scope"'),

        # Responsibility shifting
        (r'\byou will need to\b', 'Shifting: "you will need to"'),
        (r'\byou\'ll need to\b', 'Shifting: "you\'ll need to"'),
        (r'\bdon\'t forget to\b', 'Shifting: "don\'t forget to"'),
        (r'\bmake sure to\b', 'Shifting: "make sure to"'),

        # Minimization
        (r'\bbasic implementation\b', 'Minimization: "basic implementation"'),
        (r'\bsimplified\s+version\b', 'Minimization: "simplified version"'),
        (r'\bfor now\b(?!,?\s*I)', 'Minimization: "for now"'),  # Allow "for now, I will"
        (r'\buse a\s+(reasonable\s+)?default\b', 'Minimization: "use a default"'),
        (r'\ba simple\s+approach\b', 'Minimization: "a simple approach"'),

        # Code markers (in actual code blocks)
        (r'//\s*TODO\b', 'Code marker: TODO'),
        (r'//\s*FIXME\b', 'Code marker: FIXME'),
        (r'//\s*PLACEHOLDER\b', 'Code marker: PLACEHOLDER'),
        (r'//\s*HACK\b', 'Code marker: HACK'),
        (r'//\s*XXX\b', 'Code marker: XXX'),
        (r'//\s*STUB\b', 'Code marker: STUB'),
        (r'#\s*TODO\b', 'Code marker: TODO'),
        (r'#\s*FIXME\b', 'Code marker: FIXME'),
    ]

    # Find violations
    violations = []
    for pattern, label in deferral_patterns:
        if re.search(pattern, last_response, re.IGNORECASE):
            violations.append(label)

    # If violations found, BLOCK and force retry
    if violations:
        unique_violations = list(set(violations))[:5]
        error_msg = f"CONSTITUTION VIOLATION DETECTED. You MUST fix this and retry.\nViolations: {', '.join(unique_violations)}\n\nYou are PROHIBITED from deferring requirements. Solve the problem NOW or state BLOCKER: [specific issue] and ask for a decision."
        print(error_msg, file=sys.stderr)
        sys.exit(2)  # Exit code 2 = block and force retry

    sys.exit(0)

if __name__ == '__main__':
    main()
