# Tool Streaming Output

<Tip>
  Stream Tool Call is a unique feature of Z.ai's latest model GLM-4.6, allowing real-time access to reasoning processes, response content, and tool call information during tool invocation, providing better user experience and real-time feedback.
</Tip>

## Features

Tool calling in the latest GLM-4.6 model now supports streaming output for responses. This allows developers to stream tool usage parameters without buffering or JSON validation when calling `chat.completions`, reducing call latency and providing better user experience.

### Core Parameter Description

* **`stream=True`**: Enable streaming output, must be set to `True`
* **`tool_stream=True`**: Enable tool call streaming output
* **`model`**: Use a model that supports tool calling, limited to `glm-4.6`

### Response Parameter Description

The `delta` object in streaming responses contains the following fields:

* **`reasoning_content`**: Text content of the model's reasoning process
* **`content`**: Text content of the model's response
* **`tool_calls`**: Tool call information, including function names and parameters

## Code Examples

By setting the `tool_stream=True` parameter, you can enable streaming tool call functionality:

<Tabs>
  <Tab title="Python SDK">
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

    # Create streaming tool call request
    response = client.chat.completions.create(
        model="glm-4.7",  # Use model that supports tool calling
        messages=[
            {"role": "user", "content": "How's the weather in Beijing?"},
        ],
        tools=[
            {
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get current weather conditions for a specified location",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {"type": "string", "description": "City, e.g.: Beijing, Shanghai"},
                            "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
                        },
                        "required": ["location"]
                    }
                }
            }
        ],
        stream=True,        # Enable streaming output
        tool_stream=True    # Enable tool call streaming output
    )

    # Initialize variables to collect streaming data
    reasoning_content = ""      # Reasoning process content
    content = ""               # Response content
    final_tool_calls = {}      # Tool call information
    reasoning_started = False  # Reasoning process start flag
    content_started = False    # Content output start flag

    # Process streaming response
    for chunk in response:
        if not chunk.choices:
            continue

        delta = chunk.choices[0].delta

        # Handle streaming reasoning process output
        if hasattr(delta, 'reasoning_content') and delta.reasoning_content:
            if not reasoning_started and delta.reasoning_content.strip():
                print("\n🧠 Thinking Process:")
                reasoning_started = True
            reasoning_content += delta.reasoning_content
            print(delta.reasoning_content, end="", flush=True)

        # Handle streaming response content output
        if hasattr(delta, 'content') and delta.content:
            if not content_started and delta.content.strip():
                print("\n\n💬 Response Content:")
                content_started = True
            content += delta.content
            print(delta.content, end="", flush=True)

        # Handle streaming tool call information
        if delta.tool_calls:
            for tool_call in delta.tool_calls:
                index = tool_call.index
                if index not in final_tool_calls:
                    # New tool call
                    final_tool_calls[index] = tool_call
                    final_tool_calls[index].function.arguments = tool_call.function.arguments
                else:
                    # Append tool call parameters (streaming construction)
                    final_tool_calls[index].function.arguments += tool_call.function.arguments

    # Output final tool call information
    if final_tool_calls:
        print("\n📋 Function Calls Triggered:")
        for index, tool_call in final_tool_calls.items():
            print(f"  {index}: Function Name: {tool_call.function.name}, Parameters: {tool_call.function.arguments}")
    ```
  </Tab>
</Tabs>

## Application Scenarios

<CardGroup cols={2}>
  <Card title="Intelligent Customer Service" icon="headset">
    * Real-time query progress display
    * Improved waiting experience
  </Card>

  <Card title="Code Assistant" icon="code">
    * Real-time code analysis process
    * Display tool call chains
  </Card>
</CardGroup>


---

> To find navigation and other pages in this documentation, fetch the llms.txt file at: https://docs.z.ai/llms.txt
