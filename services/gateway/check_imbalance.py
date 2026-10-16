#!/usr/bin/env python3
import sys

def find_imbalance(path):
    with open(path, 'r', encoding='utf-8', errors='replace') as f:
        lines = f.readlines()

    balance = 0
    in_string = False
    in_char = False
    in_line_comment = False
    in_block_comment = False
    open_braces = []
    first_negative_at = None

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
                if balance < 0 and first_negative_at is None:
                    first_negative_at = i
                if open_braces:
                    open_braces.pop()
            j += 1
        in_line_comment = False
        # Report balance at certain key lines
        if i in (16192, 18584, 22020, 25154, 25608, 25827, 26432, 26429, 27146, 27147):
            print(f'Line {i}: balance={balance}, first_negative_at={first_negative_at}')

    print(f'\nFinal balance: {balance}')
    print(f'First negative at line: {first_negative_at}')

if __name__ == '__main__':
    find_imbalance(sys.argv[1] if len(sys.argv) > 1 else 'src/main.rs')
