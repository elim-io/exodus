## Google's skills and documentation

Google publishes Agent Skills for its products and the Developer Knowledge MCP server, which answers from Google's
current documentation. Cloud Run changes often, so use them alongside Exodus: `exodus kb` and `exodus ask` first for the
move, the prices and the plan; Google's skills and the MCP for any Cloud Run or `gcloud` fact the knowledge base does
not answer. Exodus does not install them, and neither do you: they are installers, like Exodus's own.

Once per machine, check whether they are there ({{check}}). If not, tell the person what they are and give them the
commands to run in their terminal{{bang}}:

{{install}}

The MCP server needs an API key for the Developer Knowledge API in one of the person's Google Cloud projects:
`gcloud services enable developerknowledge.googleapis.com --project=PROJECT_ID`, then a key restricted to that API, as
https://developers.google.com/knowledge/mcp shows; both change their project, so both are theirs to do. The person
puts the key in the environment variable `DEVELOPERKNOWLEDGE_API_KEY` in their shell profile and restarts {{agent}}.
It never goes on a command line, in a file of this repository or in this chat. {{mcp}} Without the key, the skills
still work. Go on with the audit whether or not they install anything.
