#!/usr/bin/env python3
"""Track brace balance in main.rs, skipping strings/chars/comments."""
import sys

def check_balance(path):
    with open(path, 'r', encoding='utf-8', errors='replace') as f:
        lines = f.readlines()

    balance = 0
    in_string = False
    in_char = False
    in_line_comment = False
    in_block_comment = False
    open_braces = []  # stack of (line, col)

    for i, line in enumerate(lines, 1):
        j = 0
        while j < len(line):
            c = line[j]
            nc = line[j+1] if j+1 < len(line) else ''
            if in_line_comment:
                break
            elif in_block_comment:
                if c == '*' and nc == '/':
                    in_block_comment = False
                    j += 2
                    continue
                j += 1
                continue
            elif in_string:
                if c == '\\':
                    j += 2
                    continue
                if c == '"':
                    in_string = False
                j += 1
                continue
            elif in_char:
                if c == '\\':
                    j += 2
                    continue
                if c == "'":
                    in_char = False
                j += 1
                continue

            if c == '/' and nc == '/':
                in_line_comment = True
                j += 2
                continue
            elif c == '/' and nc == '*':
                in_block_comment = True
                j += 2
                continue
            elif c == '"':
                in_string = True
                j += 1
                continue
            elif c == "'":
                in_char = True
                j += 1
                continue
            elif c == '{':
                balance += 1
                open_braces.append((i, j))
            elif c == '}':
                balance -= 1
                if open_braces:
                    open_braces.pop()
            j += 1
        in_line_comment = False

    print(f'Final balance: {balance}')
    print(f'Unclosed braces (last 10): {open_braces[-10:]}')
    return balance

if __name__ == '__main__':
    check_balance(sys.argv[1] if len(sys.argv) > 1 else 'src/main.rs')
