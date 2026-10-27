## Google's skills and documentation

Google publishes Agent Skills for its products and the Developer Knowledge MCP server, which answers from Google's
current documentation. Cloud Run changes often, so use them alongside Exodus: `exodus kb` and `exodus ask` first for the
move, the prices and the plan; Google's skills and the MCP for any Cloud Run or `gcloud` fact the knowledge base does
not answer. Exodus does not install them.

Once per machine, check whether they are there ({{check}}). If not, tell the person what they are and ask whether to
install them; only on a yes, run:

{{install}}

The MCP server needs an API key for the Developer Knowledge API in one of the person's Google Cloud projects:
`gcloud services enable developerknowledge.googleapis.com --project=PROJECT_ID`, then a key restricted to that API, as
https://developers.google.com/knowledge/mcp shows. Enabling the API and creating the key change their project, so they
are the person's yes too. The key lives only in the environment variable `DEVELOPERKNOWLEDGE_API_KEY`, which the person
sets in their shell profile and restarts {{agent}} after: never on a command line, in a file of this repository or in
this chat. {{mcp}} Without the key, the skills still work.
