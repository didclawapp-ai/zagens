# Vision (`describe_image`)

The **`describe_image`** tool lets the agent analyze images you attach or reference by path.

## Requirements

- **Code** or **Office** mode (when vision feature is enabled)
- `VISION_API_KEY` or provider config supporting image models (see `config.toml`)
- Image within size limits enforced by the runtime

## Typical uses

- Screenshot UI bugs → describe layout and suggest fixes
- Diagram or whiteboard photo → extract structure
- Scan a chart in `inbox/` for an Office summary

## How to invoke

- Drag/drop or paste an image into chat, or
- Place an image under workspace and ask the agent to read it

The model receives a text description from the vision backend, then continues reasoning with other tools.

## Privacy

Images are sent to your configured vision provider — treat sensitive screenshots accordingly. Do not attach credentials or personal ID photos unless you accept that risk.

Related: [File tools](/docs/tools/files) · [API key](/docs/settings/api-key)
