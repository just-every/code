export type CodexConfigValue = string | number | boolean | CodexConfigValue[] | CodexConfigObject;

export type CodexConfigObject = { [key: string]: CodexConfigValue };

export type CodexOptions = {
  codexPathOverride?: string;
  baseUrl?: string;
  apiKey?: string;
};
