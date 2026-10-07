---
title: Broken runbook (MALFORMED on purpose, issue #74)
aliases: [Broken, [nested, [deeper
tags:
  - on-call
  - [unterminated
  -
key without colon
  - "unbalanced quote

# Broken runbook

This note is malformed on purpose: the frontmatter above never closes, a code
fence below never closes, wiki links are unbalanced and nested, headings are
empty or made of markup only, and HTML blocks are left open. The parser must
return a syntax error or a partial result whose ranges stay inside the file —
never panic.

## Unbalanced links

[[ [[ [[operations/runbook#Triage]] ]] ]]
[[unterminated link to nowhere
[[#]] [[|]] [[#|alias]] [[  ]] ![[ ]] ![[
[[../../../../../../../../etc/passwd]]
[[a#b#c#d|e|f]]
]]]]]] [[[[[[ ]]

##

###### 

####### seven hashes is not a heading

#tag#tag ##double #-dash #/slash #_ #🙂 #ünïcödé #1234 #12ab
https://example.com/page#not-a-tag and mailto:a@b.c#nope

## HTML left open

<div class="callout">
<details><summary>Click

<table>
<tr><td>[[inside-html-table]]</td>

<!-- a comment that never closes

## A heading inside a comment?

Setext with nothing above
---

===

Text right under an empty setext
=

## Lists gone wrong

- item
    - deeply
        - nested
            - list
                - with [[a-link]] at depth five
                    - and #a-tag at depth six
1. ordered
3. skipped
2. backwards
-
*
+

> quote
>> nested quote with [[quoted-link]]
>>> deeper
> back to one
>>>>>>>>>> ten levels

## Tables gone wrong

| a | b |
|---|
| 1 | 2 | 3 | 4 |
| [[link-in-a-cell]] |
|||||

## Reference-style links

[undefined reference][nowhere]
[defined]: <https://example.com "title with ] bracket"
[^footnote-without-definition]
[^1]: a footnote definition [[with-a-link]]

## Emphasis soup

***bold italic** still open* **_mixed_ markers__ `code with [[link]]
``double backtick with ` inside`` and ``` triple

## Fence that never closes

```rust
fn main() {
    // [[not-a-link]] #not-a-tag
    println!("{}", "the fence below is missing");
}

## This heading is inside the open fence

[[also-not-a-link]]

- 1
- 2
- 3
- 4
- 5
- 6
- 7
- 8
- 9
- 10

line 1 of filler prose inside the fence, which keeps the file large enough
line 2: the harness requires a malformed file of at least three hundred lines
line 3: so this tail repeats realistic but broken structures
line 4

## Repeated broken sections

### Section 1
[[broken-1 | alias ]] [[ broken-1#Heading ]] #broken1
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 2
[[broken-2 | alias ]] [[ broken-2#Heading ]] #broken2
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 3
[[broken-3 | alias ]] [[ broken-3#Heading ]] #broken3
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 4
[[broken-4 | alias ]] [[ broken-4#Heading ]] #broken4
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 5
[[broken-5 | alias ]] [[ broken-5#Heading ]] #broken5
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 6
[[broken-6 | alias ]] [[ broken-6#Heading ]] #broken6
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 7
[[broken-7 | alias ]] [[ broken-7#Heading ]] #broken7
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 8
[[broken-8 | alias ]] [[ broken-8#Heading ]] #broken8
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 9
[[broken-9 | alias ]] [[ broken-9#Heading ]] #broken9
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 10
[[broken-10 | alias ]] [[ broken-10#Heading ]] #broken10
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 11
[[broken-11 | alias ]] [[ broken-11#Heading ]] #broken11
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 12
[[broken-12 | alias ]] [[ broken-12#Heading ]] #broken12
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 13
[[broken-13 | alias ]] [[ broken-13#Heading ]] #broken13
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 14
[[broken-14 | alias ]] [[ broken-14#Heading ]] #broken14
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 15
[[broken-15 | alias ]] [[ broken-15#Heading ]] #broken15
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 16
[[broken-16 | alias ]] [[ broken-16#Heading ]] #broken16
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 17
[[broken-17 | alias ]] [[ broken-17#Heading ]] #broken17
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 18
[[broken-18 | alias ]] [[ broken-18#Heading ]] #broken18
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 19
[[broken-19 | alias ]] [[ broken-19#Heading ]] #broken19
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 20
[[broken-20 | alias ]] [[ broken-20#Heading ]] #broken20
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 21
[[broken-21 | alias ]] [[ broken-21#Heading ]] #broken21
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

### Section 22
[[broken-22 | alias ]] [[ broken-22#Heading ]] #broken22
<span>[[in-span]]</span>
> [!warning
> unterminated callout
| x |
|-|
| [[cell]] |

The file ends inside the open fence, without a trailing newline.