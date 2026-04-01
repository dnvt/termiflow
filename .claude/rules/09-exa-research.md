# Exa MCP for Internet Research

When performing internet research (web searches, paper lookups, company
analysis, competitor research, technical documentation), **prefer Exa MCP tools
over built-in WebSearch/WebFetch** when the Exa MCP server is available.

## Tool Priority

1. **Exa MCP tools** (primary): `web_search_exa`, `research_paper_search`,
   `company_research_exa`, `crawling_exa`, `competitor_finder_exa`
2. **Built-in WebSearch** (fallback): Use only if Exa is unavailable or for
   quick single-query lookups where Exa would be overkill
3. **Built-in WebFetch** (supplement): For fetching specific URLs after Exa
   identifies them

## When to Use Exa

- `/maestro:pulse` research intelligence runs
- `ingest-research` skill for deep analysis
- Investor/company research for outreach
- Technical paper discovery (SLT, Interspeech, ML conferences)
- Competitive landscape analysis
- Grant program research
- Any research task requiring high-quality, curated results

## Configuration

MCP server configured in `.mcp/config.json` with `EXA_API_KEY` env var. API key
from [dashboard.exa.ai](https://dashboard.exa.ai/api-keys).
