# Streaming Messages

<Tip>
  Streaming Messages allow real-time content retrieval while the model generates responses, without waiting for the complete response to be generated. This approach can significantly improve user experience, especially when generating long text content, as users can immediately see output beginning to appear.
</Tip>

## Features

Streaming messages use an incremental generation mechanism, transmitting content in chunks in real-time during the generation process, rather than waiting for the complete response to be generated before returning it all at once. This mechanism allows developers to:

* **Real-time Response**: No need to wait for complete response, content displays progressively
* **Improved Experience**: Reduce user waiting time, provide instant feedback
* **Reduced Latency**: Content is transmitted as it's generated, reducing perceived latency
* **Flexible Processing**: Real-time processing and display during reception

### Core Parameter Description

* **`stream=True`**: Enable streaming output, must be set to `True`
* **`model`**: Models that support streaming output, such as `glm-4.7`, `glm-4.6`, `glm-4.5`, etc.

### Response Format Description

Streaming responses use Server-Sent Events (SSE) format, with each event containing:

* `choices[0].delta.content`: Incremental text content
* `choices[0].delta.reasoning_content`: Incremental reasoning content
* `choices[0].finish_reason`: Completion reason (only appears in the last chunk)
* `usage`: Token usage statistics (only appears in the last chunk)

## Code Examples

<Tabs>
  <Tab title="cURL">
    ```bash  theme={null}
    curl --location 'https://api.z.ai/api/paas/v4/chat/completions' \
    --header 'Authorization: Bearer YOUR_API_KEY' \
    --header 'Content-Type: application/json' \
    --data '{
        "model": "glm-4.7",
        "messages": [
            {
                "role": "user",
                "content": "Write a poem about spring"
            }
        ],
        "stream": true
    }'
    ```
  </Tab>

  <Tab title="Python">
    **Install SDK**

    ```bash  theme={null}
    # Install latest version
    pip install zai-sdk

    # Or specify version
    pip install zai-sdk==0.1.0
    ```

    **Verify Installation**

    ```python  theme={null}
    import zai
    print(zai.__version__)
    ```

    **Complete Example**

    ```python  theme={null}
    from zai import ZaiClient

    # Initialize client
    client = ZaiClient(api_key='Your API Key')

    # Create streaming message request
    response = client.chat.completions.create(
        model="glm-4.7",
        messages=[
            {"role": "user", "content": "Write a poem about spring"}
        ],
        stream=True  # Enable streaming output
    )

    # Process streaming response
    full_content = ""
    for chunk in response:
        if not chunk.choices:
            continue

        delta = chunk.choices[0].delta

        # Handle incremental content
        if hasattr(delta, 'content') and delta.content:
            full_content += delta.content
            print(delta.content, end="", flush=True)

        # Check if completed
        if chunk.choices[0].finish_reason:
            print(f"\n\nCompletion reason: {chunk.choices[0].finish_reason}")
            if hasattr(chunk, 'usage') and chunk.usage:
                print(f"Token usage: Input {chunk.usage.prompt_tokens}, Output {chunk.usage.completion_tokens}")

    print(f"\n\nComplete content:\n{full_content}")
    ```
  </Tab>
</Tabs>

### Response Example

The streaming response format is as follows:

```
data: {"id":"1","created":1677652288,"model":"glm-4.7","choices":[{"index":0,"delta":{"content":"Spring"},"finish_reason":null}]}

data: {"id":"1","created":1677652288,"model":"glm-4.7","choices":[{"index":0,"delta":{"content":" comes"},"finish_reason":null}]}

data: {"id":"1","created":1677652288,"model":"glm-4.7","choices":[{"index":0,"delta":{"content":" with"},"finish_reason":null}]}

...

data: {"id":"1","created":1677652288,"model":"glm-4.7","choices":[{"index":0,"finish_reason":"stop","delta":{"role":"assistant","content":""}}],"usage":{"prompt_tokens":8,"completion_tokens":262,"total_tokens":270,"prompt_tokens_details":{"cached_tokens":0}}}

data: [DONE]
```

## Application Scenarios

<CardGroup cols={2}>
  <Card title="Chat Applications" icon="headset">
    * Real-time conversation experience
    * Character-by-character reply display
    * Reduced waiting time
  </Card>

  <Card title="Content Generation" icon="feather">
    * Article writing assistant
    * Code generation tools
    * Creative content creation
  </Card>

  <Card title="Educational Applications" icon="book">
    * Online Q\&A systems
    * Learning assistance tools
    * Knowledge Q\&A platforms
  </Card>

  <Card title="Customer Service Systems" icon="users">
    * Intelligent customer service bots
    * Real-time problem solving
    * User support systems
  </Card>
</CardGroup>


---

> To find navigation and other pages in this documentation, fetch the llms.txt file at: https://docs.z.ai/llms.txt
