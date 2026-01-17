# Introduction

<Info>
  The API reference describes the RESTful APIs you can use to interact with the Z.AI platform.
</Info>

Z.AI provides standard HTTP API interfaces that support multiple programming languages and development environments, with [SDKs](/guides/develop/python/introduction) also available.

## API Endpoint

Z.ai Platform's general API endpoint is as follows:

```
https://api.z.ai/api/paas/v4
```

<Warning>
  Note: When using the [GLM Coding Plan](/devpack/overview), you need to configure the dedicated \
  Coding endpoint - [https://api.z.ai/api/coding/paas/v4](https://api.z.ai/api/coding/paas/v4) \
  instead of the general endpoint - [https://api.z.ai/api/paas/v4](https://api.z.ai/api/paas/v4) \
  Note: The Coding API endpoint is only for Coding scenarios and is not applicable to general API scenarios. Please use them accordingly.
</Warning>

## Authentication

The Z.AI API uses the standard **HTTP Bearer** for authentication.
An API key is required, which you can create or manage on the [API Keys Page](https://z.ai/manage-apikey/apikey-list).

API keys should be provided via HTTP Bearer Authentication in HTTP Request Headers.

```
Authorization: Bearer ZAI_API_KEY
```

## Playground

The API Playground allows developers to quickly try out API calls. Simply click **Try it** on the API details page to get started.

* On the API details page, there are many interactive options, such as **switching input types**, **switching tabs**, and **adding new content**.
* You can click **Add an item** or **Add new property** to add more properties the API need.
* **Note** that when switching the tabs, the previous properties value you need re-input or re-switch.

## Call Examples

<Tabs>
  <Tab title="cURL">
    ```bash  theme={null}
    curl -X POST "https://api.z.ai/api/paas/v4/chat/completions" \
    -H "Content-Type: application/json" \
    -H "Accept-Language: en-US,en" \
    -H "Authorization: Bearer YOUR_API_KEY" \
    -d '{
        "model": "glm-4.7",
        "messages": [
            {
                "role": "system",
                "content": "You are a helpful AI assistant."
            },
            {
                "role": "user",
                "content": "Hello, please introduce yourself."
            }
        ],
        "temperature": 1.0,
        "stream": true
    }'
    ```
  </Tab>

  <Tab title="Official Python SDK">
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

    **Usage Example**

    ```python  theme={null}
    from zai import ZaiClient

    # Initialize client
    client = ZaiClient(api_key="YOUR_API_KEY")

    # Create chat completion request
    response = client.chat.completions.create(
        model="glm-4.7",
        messages=[
            {
                "role": "system",
                "content": "You are a helpful AI assistant."
            },
            {
                "role": "user",
                "content": "Hello, please introduce yourself."
            }
        ]
    )

    # Get response
    print(response.choices[0].message.content)
    ```
  </Tab>

  <Tab title="Official Java SDK">
    **Install SDK**

    **Maven**

    ```xml  theme={null}
    <dependency>
        <groupId>ai.z.openapi</groupId>
        <artifactId>zai-sdk</artifactId>
        <version>0.3.0</version>
    </dependency>
    ```

    **Gradle (Groovy)**

    ```groovy  theme={null}
    implementation 'ai.z.openapi:zai-sdk:0.3.0'
    ```

    **Usage Example**

    ```java  theme={null}
    import ai.z.openapi.ZaiClient;
    import ai.z.openapi.service.model.*;
    import java.util.Arrays;

    public class QuickStart {
        public static void main(String[] args) {
            // Initialize client
            ZaiClient client = ZaiClient.builder().ofZAI()
                .apiKey("YOUR_API_KEY")
                .build();

            // Create chat completion request
            ChatCompletionCreateParams request = ChatCompletionCreateParams.builder()
                .model("glm-4.7")
                .messages(Arrays.asList(
                    ChatMessage.builder()
                        .role(ChatMessageRole.USER.value())
                        .content("Hello, who are you?")
                        .build()
                ))
                .stream(false)
                .build();

            // Send request
            ChatCompletionResponse response = client.chat().createChatCompletion(request);

            // Get response
            System.out.println(response.getData().getChoices().get(0).getMessage().getContent());
        }
    }
    ```
  </Tab>

  <Tab title="OpenAI Python SDK">
    **Install SDK**

    ```bash  theme={null}
    # Install or upgrade to latest version
    pip install --upgrade 'openai>=1.0'
    ```

    **Verify Installation**

    ```python  theme={null}
    python -c "import openai; print(openai.__version__)"
    ```

    **Usage Example**

    ```python  theme={null}
    from openai import OpenAI

    client = OpenAI(
        api_key="your-Z.AI-api-key",
        base_url="https://api.z.ai/api/paas/v4/"
    )

    completion = client.chat.completions.create(
        model="glm-4.7",
        messages=[
            {"role": "system", "content": "You are a smart and creative novelist"},
            {"role": "user", "content": "Please write a short fairy tale story as a fairy tale master"}
        ]
    )

    print(completion.choices[0].message.content)
    ```
  </Tab>

  <Tab title="OpenAI NodeJs SDK">
    **Install SDK**

    ```bash  theme={null}
    # Install or upgrade to latest version
    npm install openai

    # Or using yarn
    yarn add openai
    ```

    **Usage Example**

    ```javascript  theme={null}
    import OpenAI from "openai";

    const client = new OpenAI({
        apiKey: "your-Z.AI-api-key",
        baseURL: "https://api.z.ai/api/paas/v4/"
    });

    async function main() {
        const completion = await client.chat.completions.create({
            model: "glm-4.7",
            messages: [
                { role: "system", content: "You are a helpful AI assistant." },
                { role: "user", content: "Hello, please introduce yourself." }
            ]
        });

        console.log(completion.choices[0].message.content);
    }

    main();
    ```
  </Tab>

  <Tab title="OpenAI Java SDK">
    **Install SDK**

    **Maven**

    ```xml  theme={null}
    <dependency>
        <groupId>com.openai</groupId>
        <artifactId>openai-java</artifactId>
        <version>2.20.1</version>
    </dependency>
    ```

    **Gradle (Groovy)**

    ```groovy  theme={null}
    implementation 'com.openai:openai-java:2.20.1'
    ```

    **Usage Example**

    ```java  theme={null}
    import com.openai.client.OpenAIClient;
    import com.openai.client.okhttp.OpenAIOkHttpClient;
    import com.openai.models.chat.completions.ChatCompletion;
    import com.openai.models.chat.completions.ChatCompletionCreateParams;

    public class QuickStart {
        public static void main(String[] args) {
            // Initialize client
            OpenAIClient client = OpenAIOkHttpClient.builder()
                .apiKey("your-Z.AI-api-key")
                .baseUrl("https://api.z.ai/api/paas/v4/")
                .build();

            // Create chat completion request
            ChatCompletionCreateParams params = ChatCompletionCreateParams.builder()
                .addSystemMessage("You are a helpful AI assistant.")
                .addUserMessage("Hello, please introduce yourself.")
                .model("glm-4.7")
                .build();

            // Send request and get response
            ChatCompletion chatCompletion = client.chat().completions().create(params);
            Object response = chatCompletion.choices().get(0).message().content();

            System.out.println(response);
        }
    }
    ```
  </Tab>
</Tabs>


---

> To find navigation and other pages in this documentation, fetch the llms.txt file at: https://docs.z.ai/llms.txt
