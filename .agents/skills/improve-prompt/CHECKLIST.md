# Prompt Improvement Checklist

<instructions>
Work through each item systematically. For each technique, determine:
1. Is it present in the current prompt?
2. Would adding/improving it benefit this specific prompt?
3. If yes, how should it be applied?

Use the `<thinking>` structure from SKILL.md to document your evaluation of each technique.
</instructions>

---

<technique name="xml_tags" number="1" type="automated">
## 1. XML Tags (Run Script)

**Script:** `python {{scripts_dir}}/checks/xml_tags.py {{temp_prompt_file}}`

**After running script, evaluate:**

*If tags found:*
- Do they cover all distinct content types in the prompt? (instructions, context, input, examples, output format, constraints)
- Are any tags unnecessary? (wrapping content that doesn't need boundaries)
- Are tag names descriptive and meaningful for the content they surround?
- Is the prompt consistent? (same tag names used throughout, tags referenced in instructions e.g., "Using the code in `<code>` tags...")
- Should any tags be nested for hierarchical content?
- For multiple documents: are they individually wrapped with metadata subtags like `<document_content>` and `<source>`?

*If no tags found:*
- Apply "When to add" criteria below
- If criteria met, determine which specific tags would benefit the prompt

**When to add XML tags:**
- Prompt has multiple distinct sections
- There's input content that needs boundaries (user data, documents, code)
- Multiple output sections expected
- Prompt exceeds 3-4 sentences

**Common useful tags:**
- `<instructions>` - The actual task/request
- `<context>` - Background information
- `<input>` or `<document>` - User-provided content
- `<examples>` - Example input/output pairs
- `<output_format>` - Expected response structure
- `<constraints>` - Limitations and rules

**Best practices (from Anthropic):**
- Be consistent: use the same tag names throughout and refer to them in instructions
- Nest tags for hierarchical content: `<outer><inner></inner></outer>`
- Tag names should make sense with the information they surround
- For long documents (20K+ tokens): place at top of prompt, above instructions

**Skip if:** Very short, single-purpose prompts (1-2 sentences)
</technique>

---

<technique name="variables" number="2" type="automated">
## 2. Variables (Run Script)

**Script:** `python {{scripts_dir}}/checks/variables.py {{temp_prompt_file}}`

**After running script, evaluate:**

*If variables found:*
- Do they cover all dynamic/injectable content in the prompt?
- Are any variables unnecessary? (content that will actually be static)
- Are variable names descriptive and self-documenting? (e.g., `{{user_query}}` not `{{input}}`)
- Is the variable format consistent throughout? (don't mix `{{var}}` with `${var}`)
- Does the prompt mention content that should be a variable but isn't? (e.g., "the code below" without a variable)

*If no variables found:*
- Apply "When to add" criteria below
- Look for phrases like "the document", "the code", "the user's input" that indicate dynamic content
- Check if this prompt will be reused with different inputs

**When to add variables:**
- Prompt is a template for reuse
- Dynamic content will be injected
- There are placeholder descriptions like "insert X here"

**Standard format:** `{{variable_name}}`

**Skip if:** One-off prompt, no dynamic content needed
</technique>

---

<technique name="role" number="3" type="manual">
## 3. Role / Persona

**Look for:** "You are", "Act as", "Your role", or persona descriptions

**After reviewing prompt, evaluate:**

*If role found:*
- Is it specific enough? ("data scientist" vs "data scientist specializing in customer insight analysis for Fortune 500 companies")
- Is it in the right place? (should be in `system` parameter, not `user` message)
- Does it match the task's domain and expertise needs?
- Is the communication style appropriate for the audience?

*If no role found:*
- Apply "When to add" criteria below
- Consider what expertise would most benefit the task

**When to add a role:**
- Task requires specific expertise (coding, medical, legal, scientific)
- Specific communication style needed (formal, casual, technical)
- Domain knowledge should be emphasized
- Perspective matters (teacher explaining to student, consultant to client)

**Good role examples:**
- "You are a senior Python developer reviewing code for security issues"
- "You are a patient teacher explaining concepts to a beginner"
- "You are a data analyst presenting findings to executives"

**Best practices (from Anthropic):**
- Use the `system` parameter for role; put task instructions in `user` turn
- Experiment with specificity - more specific roles often yield better results
- The right role can turn Claude from a general assistant into a domain expert

**Skip if:**
- Simple factual tasks
- Creative tasks where persona might constrain
- General-purpose assistance
</technique>

---

<technique name="examples" number="4" type="manual">
## 4. Examples (Multishot)

**Look for:** "Example:", "For example", input/output pairs, numbered demonstrations

**After reviewing prompt, evaluate:**

*If examples found:*
- Are there enough? (3-5 diverse examples optimal for complex tasks)
- Are they relevant to the actual use case?
- Are they diverse? (cover edge cases, vary enough to avoid unintended patterns)
- Are they clearly structured? (wrapped in `<example>` tags, or nested in `<examples>`)
- Do they show complete input → output transformations?
- Could Claude help evaluate or generate more examples?

*If no examples found:*
- Apply "When to add" criteria below
- Consider if the task is ambiguous enough to benefit from demonstration

**When to add examples:**
- Output format is specific or unusual
- Task is ambiguous - examples clarify expectations
- Classification or categorization tasks
- Edge cases need to be demonstrated
- Consistent style/format is critical

**Best practices (from Anthropic):**
- 3-5 diverse, relevant examples for complex tasks (more examples = better performance)
- Wrap examples in `<example>` tags; nest multiple in `<examples>` for structure
- Include at least one edge case
- Vary examples to avoid Claude picking up unintended patterns
- Ask Claude to evaluate your examples for relevance, diversity, or clarity
- Ask Claude to generate more examples based on your initial set

**Skip if:**
- Task is straightforward and well-defined
- Output format is natural language without specific structure
- Examples would be redundant given clear instructions
</technique>

---

<technique name="chain_of_thought" number="5" type="manual">
## 5. Chain of Thought

**Look for:** "step by step", "think through", "show reasoning", "explain your logic", `<thinking>` tags

**After reviewing prompt, evaluate:**

*If CoT prompting found:*
- What level is it? (basic/guided/structured - see below)
- Does it require Claude to OUTPUT its thinking? (Critical: no output = no thinking!)
- Is the level appropriate for task complexity?
- For structured CoT: are `<thinking>` and `<answer>` tags specified?
- Could it benefit from more specific step guidance?

*If no CoT found:*
- Apply "When to add" criteria below
- Consider: would a human need to think through this task?
- Weigh accuracy benefit against latency cost

**When to add chain of thought:**
- Multi-step reasoning or calculation
- Logic puzzles or math problems
- Complex analysis or decision-making
- Debugging or troubleshooting
- Tasks where accuracy is critical
- Comparative analysis

**Three levels of CoT (from Anthropic, least to most complex):**
1. **Basic**: "Think step-by-step" - simple but lacks guidance on *how* to think
2. **Guided**: Outline specific steps for Claude to follow in its thinking
3. **Structured**: Use `<thinking>` and `<answer>` tags to separate reasoning from final answer

**Phrases to add:**
- "Think through this step by step"
- "Show your reasoning before giving a final answer"
- "Break this problem down into steps"
- "First analyze X, then consider Y, finally conclude Z"

**Critical (from Anthropic):** Always have Claude output its thinking. Without outputting its thought process, no thinking occurs!

**Skip if:**
- Simple, direct tasks (summarize, translate, format)
- Creative writing where reasoning would interrupt flow
- Speed/latency is priority over explanation
</technique>

---

<technique name="clarity" number="6" type="manual">
## 6. Clarity and Specificity

**Look for vague language:**
- "good", "bad", "better", "appropriate", "proper"
- "a few", "some", "many", "several"
- "short", "long", "brief", "detailed"
- "etc.", "and so on"
- "if needed", "as appropriate"

**How to improve:**
- Replace vague quantities: "a few" → "3-5"
- Replace vague quality: "good summary" → "summary covering main points, under 100 words"
- Replace vague length: "short response" → "2-3 sentences"
- Replace vague standards: "appropriate tone" → "professional but approachable"

**Every prompt should specify:**
- What exactly to do (action)
- What to include/exclude (scope)
- How long/detailed (length)
- In what format (structure)
</technique>

---

<technique name="output_format" number="7" type="manual">
## 7. Output Format

**Look for:** JSON, markdown, bullet points, tables, specific structure

**After reviewing prompt, evaluate:**

*If format specified:*
- Is it specific enough for the use case?
- Will the output be parsed programmatically? If so, is the format machine-readable?
- Are all required fields/sections explicitly listed?
- Would prefilling help enforce the format?

*If no format specified:*
- Apply "When to specify" criteria below
- Consider how the output will be used (human reading vs programmatic parsing)

**When to specify format:**
- Output will be parsed programmatically
- Specific sections or headings needed
- Consistent structure across multiple uses
- Tables or structured data expected

**How to specify:**
- "Respond in JSON with keys: title, summary, tags"
- "Use markdown with ## headers for each section"
- "Format as a numbered list"
- "Structure your response as: 1) Analysis 2) Recommendation 3) Next steps"

**Best practices (from Anthropic):**
- Prefill `{` to force JSON output and skip preamble (cleaner, easier to parse)
- For guaranteed JSON conforming to a schema, consider using Structured Outputs API instead of prefilling
- Prefilling cannot end with trailing whitespace
- Note: Prefilling is not supported with extended thinking mode
</technique>

---

<technique name="constraints" number="8" type="manual">
## 8. Constraints and Boundaries

**Look for:** "do not", "avoid", "only", "must", "never", "exclude"

**After reviewing prompt, evaluate:**

*If constraints found:*
- Are they comprehensive? (content, format, scope, length, tone)
- Are there implicit constraints that should be made explicit?
- Could any constraints conflict with each other or the task?
- Are they specific enough? ("keep it short" vs "under 200 words")

*If no constraints found:*
- Apply "When to add" criteria below
- Consider: what could go wrong if Claude interprets this too broadly?
- Think about what you DON'T want in the output

**When to add constraints:**
- Things Claude should NOT do or include
- Topics to avoid
- Scope limitations
- Style restrictions (no jargon, no bullet points, etc.)

**Types of constraints:**
- **Content:** "Do not include personal opinions"
- **Format:** "Do not use bullet points"
- **Scope:** "Focus only on X, do not discuss Y"
- **Length:** "Keep response under 200 words"
- **Tone:** "Avoid technical jargon"

**Tip:** Negative instructions (what NOT to do) can be as important as positive ones
</technique>

---

<technique name="context" number="9" type="manual">
## 9. Context and Background

**Consider:** Does the prompt provide enough context?

**After reviewing prompt, evaluate:**

*If context found:*
- Does it cover all four elements? (audience, purpose, prior work, external constraints)
- Is any context missing that would help Claude perform better?
- Is the context placed appropriately? (for long context: at top, above instructions)
- Is extraneous context adding noise without value?

*If no context found:*
- Apply "When to add" criteria below
- Ask: what would a new employee need to know to do this task well?

**When to add context:**
- Task requires understanding background situation
- Audience for the output matters
- Prior conversation/work is relevant
- Domain-specific considerations apply

**Context elements:**
- Who will read the output?
- What's the purpose/goal?
- What has already been tried/done?
- What constraints exist externally?

**Best practices (from Anthropic):**
- Think of Claude as a brilliant but new employee with amnesia who needs explicit context
- For long documents (20K+ tokens): place at top of prompt, above instructions
- The more precisely you explain what you want, the better Claude's response
</technique>

---

<technique name="task_decomposition" number="10" type="manual">
## 10. Task Decomposition

**Consider:** Is this prompt trying to do too much?

**After reviewing prompt, evaluate:**

*Signs prompt should be split:*
- Multiple unrelated tasks in one prompt
- "First do X, then do Y, then do Z" with complex steps
- Output would be very long
- Different expertise needed for different parts
- Multiple transformations, citations, or complex instructions
- Claude is dropping steps or performing poorly on some parts

*If prompt is appropriately scoped:*
- Single, clear objective
- Steps are related and build on each other
- Reasonable output length expected

**Solution:** Break into chained prompts where output of one feeds into next

**Best practices (from Anthropic):**
- Each prompt in a chain should have a single-task goal
- Use XML tags to pass outputs between prompts for clear handoffs
- For independent subtasks, run separate prompts in parallel for speed
- Benefits: better accuracy (full attention per step), clarity, easier debugging
- Self-correction chains: have Claude review its own work in a follow-up prompt

**Example chained workflows:**
- Content creation: Research → Outline → Draft → Edit → Format
- Data processing: Extract → Transform → Analyze → Visualize
- Decision-making: Gather info → List options → Analyze each → Recommend
- Verification: Generate → Review → Refine → Re-review
</technique>

---

<final_check>
## Final Quality Check

Before finalizing the improved prompt, verify:

- [ ] Is the core task crystal clear?
- [ ] Are all vague terms replaced with specifics?
- [ ] Is the expected output format defined?
- [ ] Are constraints and boundaries explicit?
- [ ] Would examples help? If so, are they included?
- [ ] Is the prompt appropriately structured (XML tags if needed)?
- [ ] Is it concise? (Remove unnecessary words)
- [ ] Would you know exactly what to do if given this prompt?

**If any checkbox is unchecked, revisit that aspect before finalizing.**
</final_check>
