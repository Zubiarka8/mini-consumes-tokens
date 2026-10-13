# Prompt Engineering Techniques Reference

Condensed from Anthropic's official prompt engineering documentation.
Source: https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/

---

## Be Clear and Direct

**Core principle:** Treat Claude like a new employee - provide explicit context and instructions.

**Key practices:**
- State the task directly and specifically
- Provide all necessary context upfront
- Be explicit about what you want (format, length, style)
- Remove ambiguity - if something could be interpreted multiple ways, clarify

**Bad:** "Write something about climate change"
**Good:** "Write a 200-word summary of climate change causes for a high school audience, focusing on the three main contributors to greenhouse gas emissions"

---

## Multishot Prompting (Examples)

**When to use:** When output format matters or task is ambiguous.

**Best practices:**
- Use 2-5 diverse examples
- Include edge cases
- Show complete input → output pairs
- Ensure examples are representative of desired behavior

**Structure:**
```
Here are examples of the task:

Example 1:
Input: [example input]
Output: [example output]

Example 2:
Input: [example input with edge case]
Output: [example output]

Now perform the same task:
Input: [actual input]
Output:
```

---

## Chain of Thought

**When to use:** Complex reasoning, math, logic, analysis, debugging.

**How to invoke:**
- "Think through this step by step"
- "Before answering, reason through the problem"
- "Show your work"
- "Break this down into steps"

**Structure:**
```
[Task description]

Think through this step by step:
1. First, consider...
2. Then, analyze...
3. Finally, conclude...
```

**Note:** For extended thinking models, don't over-constrain the thinking process.

---

## XML Tags

**When to use:** Complex prompts with multiple sections, inputs, or when structure helps.

**Common tags:**
- `<instructions>` - What to do
- `<context>` - Background info
- `<input>` / `<document>` - User content
- `<examples>` - Sample input/output
- `<output_format>` - Expected structure
- `<constraints>` - Rules and limits

**Example:**
```xml
<instructions>
Summarize the following document, focusing on key findings.
</instructions>

<document>
{{document_content}}
</document>

<output_format>
Provide a 3-paragraph summary:
1. Main thesis
2. Key findings
3. Conclusions
</output_format>
```

---

## System Prompts / Roles

**When to use:** When expertise, tone, or perspective matters.

**Structure:** Set role at the beginning of the prompt or in system message.

**Effective roles include:**
- Expertise level: "senior developer", "expert analyst"
- Communication style: "patient teacher", "concise professional"
- Perspective: "skeptical reviewer", "supportive mentor"

**Example:**
```
You are a senior security engineer reviewing code for vulnerabilities.
You are thorough, detail-oriented, and prioritize practical risks over theoretical ones.
```

---

## Prefill Claude's Response

**When to use:** When output format must be exact (JSON, code, specific structure).

**How it works:** Start Claude's response with specific text to constrain format.

**Example for JSON:**
```
Respond with valid JSON.

{
```

**Example for specific format:**
```
Start your response with "Analysis:"
```

---

## Chain Prompts

**When to use:** Complex multi-step tasks, when one task's output feeds another.

**Strategy:**
1. Break complex task into discrete steps
2. Run each step as separate prompt
3. Use output of one as input to next

**Benefits:**
- Easier debugging
- Better accuracy per step
- Can validate intermediate outputs
- Reduces prompt complexity

---

## Long Context Tips

**When working with large amounts of text:**

1. **Put key info at start and end** - Middle content can be "lost"
2. **Use clear section markers** - Headers, XML tags, dividers
3. **Summarize key points** - Highlight what matters most
4. **Be explicit about what to focus on** - Don't make Claude search

**Structure for long docs:**
```
<task>
[What to do with the content below]
</task>

<key_points>
Focus on: X, Y, Z
</key_points>

<content>
[Long content here]
</content>
```

---

## Extended Thinking Tips

**For complex reasoning tasks with thinking-enabled models:**

1. Don't over-constrain thinking
2. Allow room for exploration
3. Focus on the problem, not the process
4. Trust the model to reason

**Avoid:** "Think for exactly 3 steps" or overly specific thinking instructions
**Better:** "Consider this carefully before responding"

---

## Quick Reference: Improvement Patterns

| Issue | Solution |
|-------|----------|
| Vague output | Add output format specification |
| Inconsistent style | Add examples showing desired style |
| Wrong focus | Add constraints about what to include/exclude |
| Too long/short | Specify length explicitly |
| Missing context | Add role and/or context section |
| Errors in reasoning | Add chain of thought |
| Format issues | Use prefill or examples |
| Complex task | Split into chained prompts |
