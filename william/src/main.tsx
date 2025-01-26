import { useState, useEffect, useCallback, useRef } from 'react';
import React from 'react';
import ReactDOM from 'react-dom/client';
import MarkdownIt from 'markdown-it';
import markdownItKatex from 'markdown-it-katex';
import { z } from 'zod';
import hljs from 'highlight.js';

import './font.css';
import './buttons.css';

const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: true,
  highlight: function(str, lang) {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return hljs.highlight(str, { language: lang }).value;
      } catch (__) { }
    }
    return ''; // use external default escaping
  }
}).use(markdownItKatex);

// TODO: There's gotta be a better way of organizing things than a series of comments
//       More meaning implicit in structure, please

const root = ReactDOM.createRoot(
  document.getElementById('root') as HTMLElement
);

interface WebSocketHookOptions {
  url: string;
  retryInterval?: number;
  maxRetries?: number;
}

interface WebSocketHookReturn {
  socket: WebSocket | null;
  setUserConfig: (userConfig: UserConfig | null) => void,
  userConfig: UserConfig | null;
  conversations: Conversation[];
  loadedConversation: Conversation;
  setLoadedConversation: Function;
  sendMessage: (message: ArrakisRequest) => void;
  connectionStatus: 'connecting' | 'connected' | 'disconnected';
  error: Error | null;
}

// A variety of types for communicating with the backend
// I think this is really all they're here for

const OpenAIModelSchema = z.enum([
  "gpt-4o",
  "gpt-4o-mini",
  "o1-preview",
  "o1-mini",
]);

const GroqModelSchema = z.enum([
  "llama3-70b-8192",
]);

const AnthropicModelSchema = z.enum([
  "claude-3-opus-20240229",
  "claude-3-sonnet-20240229",
  "claude-3-haiku-20240307",
  "claude-3-5-sonnet-latest",
  "claude-3-5-haiku-latest",
]);

const APISchema = z.discriminatedUnion("provider", [
  z.object({
    provider: z.literal("openai"),
    model: OpenAIModelSchema,
  }),
  z.object({
    provider: z.literal("groq"),
    model: GroqModelSchema,
  }),
  z.object({
    provider: z.literal("anthropic"),
    model: AnthropicModelSchema,
  }),
]);

const MessageSchema = z.object({
  message_type: z.enum(["System", "User", "Assistant"]),
  id: z.number().nullable(),
  content: z.string(),
  api: APISchema,
  system_prompt: z.string(),
  sequence: z.number(),
});

const ConversationSchema = z.object({
  id: z.number().nullable(),
  name: z.string(),
  messages: z.array(MessageSchema),
});

const CompletionRequestSchema = ConversationSchema;

const ApiKeysSchema = z.object({
  openai: z.string(),
  anthropic: z.string(),
  grok: z.string(),
  groq: z.string(),
  gemini: z.string(),
});

const UserConfigRequestSchema = z.object({
  write: z.boolean(),
  systemPrompt: z.string(),
  apiKeys: ApiKeysSchema,
});

const PingRequestSchema = z.object({
  body: z.string(),
});

const LoadRequestSchema = z.object({
  id: z.number(),
});

const ForkRequestSchema = z.object({
  conversationId: z.number(),
  sequence: z.number(),
});

const PreviewRequestSchema = z.object({
  conversationId: z.number().int(),
  content: z.string()
});

const PreviewResponseSchema = PreviewRequestSchema;

const CompletionResponseSchema = z.object({
  stream: z.boolean(),
  delta: z.string(),
  name: z.string(),
  conversationId: z.number(),
  requestId: z.number(),
  responseId: z.number(),
});

const ErrorResponseSchema = z.object({
  error_type: z.string(),
  message: z.string(),
});

const UserConfigResponseSchema = UserConfigRequestSchema;

const PingResponseSchema = PingRequestSchema;

const ConversationListResponseSchema = z.object({
  conversations: z.array(ConversationSchema),
});

const ArrakisRequestSchema = z.discriminatedUnion("method", [
  z.object({
    method: z.literal("ConversationList"),
    id: z.string().optional(),
  }),
  z.object({
    method: z.literal("Ping"),
    id: z.string().optional(),
    payload: PingRequestSchema,
  }),
  z.object({
    method: z.literal("Completion"),
    id: z.string().optional(),
    payload: CompletionRequestSchema,
  }),
  z.object({
    method: z.literal("Load"),
    id: z.string().optional(),
    payload: LoadRequestSchema,
  }),
  z.object({
    method: z.literal("Config"),
    id: z.string().optional(),
    payload: UserConfigRequestSchema,
  }),
  z.object({
    method: z.literal("Fork"),
    id: z.string().optional(),
    payload: ForkRequestSchema,
  }),
  z.object({
    method: z.literal("Preview"),
    id: z.string().optional(),
    payload: PreviewRequestSchema,
  }),
]);

const ArrakisResponseSchema = z.discriminatedUnion("method", [
  z.object({
    method: z.literal("ConversationList"),
    id: z.string(),
    payload: ConversationListResponseSchema,
  }),
  z.object({
    method: z.literal("Load"),
    id: z.string(),
    payload: ConversationSchema,
  }),
  z.object({
    method: z.literal("Ping"),
    id: z.string(),
    payload: PingResponseSchema,
  }),
  z.object({
    method: z.literal("Completion"),
    id: z.string(),
    payload: CompletionResponseSchema,
  }),
  z.object({
    method: z.literal("Config"),
    id: z.string(),
    payload: UserConfigResponseSchema,
  }),
  z.object({
    method: z.literal("WilliamError"),
    id: z.string(),
    payload: ErrorResponseSchema,
  }),
  z.object({
    method: z.literal("Preview"),
    id: z.string(),
    payload: PreviewResponseSchema,
  }),
]);

type API = z.infer<typeof APISchema>;
type Message = z.infer<typeof MessageSchema>;
type Conversation = z.infer<typeof ConversationSchema>;
type ApiKeys = z.infer<typeof ApiKeysSchema>;
type UserConfig = z.infer<typeof UserConfigRequestSchema>;
type UserConfigRequest = z.infer<typeof UserConfigRequestSchema>;
type PingRequest = z.infer<typeof PingRequestSchema>;
type LoadRequest = z.infer<typeof LoadRequestSchema>;
type ArrakisRequest = z.infer<typeof ArrakisRequestSchema>;
type ArrakisResponse = z.infer<typeof ArrakisResponseSchema>;

// TODO: disgusting mixing of concerns between this and the main page
//       should probably centralize everything dealing with message responses
//       in here + separate away from rendering

interface TitleCaseOptions {
  preserveAcronyms?: boolean;
  handleHyphens?: boolean;
  customMinorWords?: string[];
}

// We could probably just prompt GPT better instead of using this
function formatTitle(input: string, options: TitleCaseOptions = {}): string {
  if (!input) return '';

  const {
    preserveAcronyms = true,
    handleHyphens = true,
    customMinorWords = [],
  } = options;

  const MINOR_WORDS = new Set([
    'a', 'an', 'the', 'and', 'but', 'or', 'nor', 'for', 'yet', 'so',
    'in', 'on', 'at', 'to', 'for', 'of', 'with', 'by',
    ...customMinorWords
  ]);

  const isAcronym = (word: string): boolean => {
    return /^[A-Z0-9]+$/.test(word);
  };

  const capitalizeWord = (word: string, forceCapitalize: boolean = false): string => {
    if (preserveAcronyms && isAcronym(word)) {
      return word;
    }

    const wordLower = word.toLowerCase();

    if (forceCapitalize || !MINOR_WORDS.has(wordLower)) {
      return word.charAt(0).toUpperCase() + wordLower.slice(1);
    }

    return wordLower;
  };

  const processHyphenatedWord = (word: string, forceCapitalize: boolean): string => {
    if (!handleHyphens) return capitalizeWord(word, forceCapitalize);

    return word = word
      .split('-')
      .map((part, index) => capitalizeWord(part, index === 0 && forceCapitalize))
      .join('-');
  };

  const words = input.split(/\s+/);

  const capitalizedWords = words.map((word, index) => {
    const isFirst = index === 0;
    const isLast = index === words.length - 1;

    return processHyphenatedWord(word, isFirst || isLast);
  });

  return capitalizedWords.join(' ');
}

function conversationDefault(): Conversation {
  return ConversationSchema.parse({ id: null, name: crypto.randomUUID(), messages: [] });
}

// Quick and dirty hook for connecting to the backend
// TODO: this will probably need expanded in the future to accommodate better error handling and the like
// TODO: Needs a refactor for the new callback argument in `send`
const useWebSocket = ({
  url,
  retryInterval = 5000,
  maxRetries = 0
}: WebSocketHookOptions): WebSocketHookReturn => {
  const [socket, setSocket] = useState<WebSocket | null>(null);
  const [userConfig, setUserConfig] = useState<UserConfig | null>(null);
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [loadedConversation, setLoadedConversation] = useState<Conversation>(conversationDefault());
  const [connectionStatus, setConnectionStatus] = useState<'connected' | 'disconnected'>('disconnected');
  const [error, setError] = useState<Error | null>(null);
  const retryCount = useRef<number>(0);

  // A list of outstanding callbacks for outstanding requests
  // Each request going out has a callback associated under the assumption that
  // something will happen with the acquired data from the response
  //
  // TODO: There will need to be some refactoring to account for this addition
  const callbacks = useRef<{ [key: string]: (response: ArrakisResponse) => void }>({});

  // This consumes the callback associated with the given response
  // (if there is any to begin with)
  // If there isn't an associated callback, this function passes through silently
  const useResponseCallback = (response: ArrakisResponse) => {
    if (callbacks.current[response.id]) {
      callbacks.current[response.id](response);
      delete callbacks.current[response.id];
    }
  };

  const connect = useCallback(() => {
    const attemptConnection = () => {
      try {
        const ws = new WebSocket(url);

        ws.onopen = () => {
          setConnectionStatus('connected');
          setError(null);
          retryCount.current = 0;
        };

        // This is a sorry excuse for a REST-ish API
        // I feel like there's a much better way of structuring the "endpoints" supported by both the front + back ends
        //
        // TODO: Refactor because this is gross
        //       Also to use the new callback system
        ws.onmessage = (event) => {
          try {
            const response = ArrakisResponseSchema.parse(JSON.parse(event.data));
            if (response.method === 'Completion') {
              setLoadedConversation(prev => {
                const completion = CompletionResponseSchema.parse(response.payload);

                const lcm = prev.messages;
                const newMessages = [...lcm.slice(0, lcm.length - 1)];

                const last = lcm[lcm.length - 1];
                last.content += completion.delta;
                last.id = completion.responseId;

                newMessages[newMessages.length - 1].id = completion.requestId;
                newMessages.push(last);

                return { id: completion.conversationId, name: completion.name, messages: newMessages };
              });
            } else if (response.method === 'Ping' && connectionStatus !== 'connected') {
              setConnectionStatus('connected');
            } else if (response.method === 'ConversationList') {
              const conversationList = ConversationListResponseSchema.parse(response.payload);
              setConversations(conversationList.conversations);
            } else if (response.method === 'Load') {
              const conversation = ConversationSchema.parse(response.payload);
              setLoadedConversation(conversation);
            } else if (response.method === 'Config') {
              const payload = UserConfigResponseSchema.parse(response.payload);
              setUserConfig(payload);
            } else if (response.method === 'WilliamError') {
              const payload = ErrorResponseSchema.parse(response.payload);
              setError(new Error(payload.message));
            } else if (response.method === 'Preview') {
              useResponseCallback(response);
            }
          } catch (error) {
            console.log(error);
          }
        };

        ws.onclose = () => {
          setConnectionStatus('disconnected');
          setSocket(null);
        };

        setSocket(ws);
      } catch (err) {
        if (err instanceof Error) {
          err.message += `; ${retryCount.current} retries`;
        }

        setError(err instanceof Error ? err : new Error('Failed to create WebSocket connection'));

        // Retry if things fail, 3 second timer
        setTimeout(attemptConnection, 3000);
        retryCount.current += 1;
      }
    };

    attemptConnection();
  }, [url, retryCount, maxRetries, retryInterval]);

  // Generic message sending function for the backend
  // Ideally, _all_ messages going to the backend will be an ArrakisRequest
  //
  // IDs are assigned here and managed through solely within this socket hook
  // TODO: Probably a refactor to ensure they can't be tampered with outside this hook
  const sendMessage = useCallback((message: ArrakisRequest, callback?: (response: ArrakisResponse) => void) => {
    if (socket?.readyState === WebSocket.OPEN) {
      message.id = crypto.randomUUID();

      if (callback) {
        callbacks.current[message.id] = callback;
      }

      socket.send(typeof message === 'string' ? message : JSON.stringify(message));
    } else {
      console.error('WebSocket is not connected');
    }
  }, [socket]);

  // Initial socket connection
  useEffect(() => {
    connect();

    return () => {
      if (socket) {
        socket.close();
      }
    };
  }, [connect]);

  return {
    socket,
    setUserConfig,
    userConfig,
    conversations,
    loadedConversation,
    setLoadedConversation,
    sendMessage,
    connectionStatus,
    error
  };
};

// Basic converter of HTML strings to proprerly structured react elements
// needed for parsing/unescaping/markdown-ing each message's contents
//
// TODO: though I'm sure there's a better way 
type ReactElementOrText = React.ReactElement | string | null;
function htmlToReactElements(htmlString: string) {
  const parser = new DOMParser();
  const doc = parser.parseFromString(htmlString, 'text/html');

  function domToReact(node: Node): ReactElementOrText {
    if (node.nodeType === Node.TEXT_NODE) {
      return node.textContent;
    }

    if (node.nodeType === Node.ELEMENT_NODE) {
      const elementNode = node as HTMLElement;

      const tagName = (() => {
        const tn = elementNode.tagName.toLowerCase();
        return tn;
      })();

      const props: Record<string, string> = {};
      Array.from(elementNode.attributes).forEach(attr => {
        let name = attr.name;
        if (name === 'class') name = 'className';
        if (name === 'for') name = 'htmlFor';

        props[name] = attr.value;
      });

      const children = Array.from(elementNode.childNodes).map(domToReact);

      return React.createElement(tagName, props, ...children);
    }

    return null;
  }

  return Array.from(doc.body.childNodes).map(domToReact);
}

const escapeToHTML: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;'
};

const escapeFromHTML: Record<string, string> = Object.entries(escapeToHTML).reduce((acc, [key, value]) => {
  acc[value as string] = key;
  return acc;
}, {} as Record<string, string>);

// Which model belongs to which provider
// [Model]: Provider
//
// TODO: These are commented out because I haven't figured out a smarter scheme for mapping models to names
const MODEL_PROVIDER_MAPPING: Record<string, string> = {
  // "notepad": "notepad",
  "gpt-4o": "openai",
  // "gpt-4o-mini": "openai",
  "o1-preview": "openai",
  // "o1-mini": "openai",
  "llama3-70b-8192": "groq",
  // "claude-3-opus-20240229": "anthropic",
  // "claude-3-sonnet-20240229": "anthropic",
  // "claude-3-haiku-20240307": "anthropic",
  "claude-3-5-sonnet-latest": "anthropic",
  // "claude-3-5-haiku-latest": "anthropic"
};

// TODO: This needs to be better + more robust
const MODEL_LABEL_MAPPING: Record<string, string> = {
  // "notepad": "Notepad",
  "gpt-4o": "GPT",
  // "gpt-4o-mini": "openai",
  "o1-preview": "GPT (smarter)",
  // "o1-mini": "openai",
  "llama3-70b-8192": "LLaMA",
  // "claude-3-opus-20240229": "anthropic",
  // "claude-3-sonnet-20240229": "anthropic",
  // "claude-3-haiku-20240307": "anthropic",
  "claude-3-5-sonnet-latest": "Claude",
  // "claude-3-5-haiku-latest": "anthropic"
};

const menuButtonStyle: React.CSSProperties = {
  userSelect: 'none',
  cursor: 'default',
  alignSelf: 'center',
  display: 'flex',
  justifyContent: 'center',
  alignItems: 'center',
  textAlign: 'center',
  padding: '0 1.5rem',
  height: '100%',
};

// Get a list of models { model: string, provider: string }
// depending on the availability of configured API keys
function filterAvailableModels(userConfig: UserConfig | null) {
  return Object.keys(MODEL_PROVIDER_MAPPING)
    .filter(m => {
      const provider = MODEL_PROVIDER_MAPPING[m];
      return !((provider === 'openai' && userConfig?.apiKeys.openai === '') ||
        (provider === 'anthropic' && userConfig?.apiKeys.anthropic === '') ||
        (provider === 'groq' && userConfig?.apiKeys.groq === ''));
    })
    .map(m => ({ model: m, provider: MODEL_PROVIDER_MAPPING[m], }));
}

// Generic dropdown for setting the current LLM backend, which updates the main app state through props.modelCallback
const ModelDropdown = (props: { userConfig: UserConfig | null, model: string, modelCallback: Function }) => {
  const [isOpen, setIsOpen] = useState(false);
  const popupRef = useRef<HTMLDivElement | null>(null);
  const buttonRef = useRef<HTMLDivElement | null>(null);

  // Clicking outside the dropdown closes it
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent | any) => {
      if (
        popupRef.current &&
        !popupRef.current.contains(event.target as Node) &&
        buttonRef.current &&
        !buttonRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };

    // Hotkeys for opening/selecting/closing
    const handleKeyPress = (event: any) => {
      const numbers = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];
      if (event.key === 'Escape') {
        setIsOpen(false);
      } else if (event.ctrlKey && event.key === 'm') {
        setIsOpen(true);
      } else if (event.ctrlKey && numbers.includes(event.key)) {
        const models = filterAvailableModels(props.userConfig);
        const index = Math.min(parseInt(event.key) - 1, models.length - 1);

        props.modelCallback(models[index]);
        setIsOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside as any);
    document.addEventListener('keydown', handleKeyPress);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside as any);
      document.removeEventListener('keydown', handleKeyPress);
    };
  }, []);

  return (
    <div
      ref={buttonRef}
      onClick={() => setIsOpen(!isOpen)}
      className="buttonHoverLight"
      style={menuButtonStyle}>
      {MODEL_LABEL_MAPPING[props.model]}

      {
        isOpen && (
          <div
            ref={popupRef}
            className="popup-content"
          >
            {
              // We want to limit the visible model options to those API keys the user has set
              // i.e., no API key, no model option
              filterAvailableModels(props.userConfig).map(m => (
                <div
                  onClick={() => props.modelCallback(m)}
                  className="buttonHover"
                  style={{
                    textWrap: 'nowrap',
                    padding: '0.5rem',
                  }}>
                  {MODEL_LABEL_MAPPING[m.model]}
                </div>
              ))
            }
          </div>
        )
      }
    </div >
  );
};

// TODO: Take another look at how this is being used,
//       I think it should probably be removed/refactored at this point
type Modal = 'config' | 'search' | null;

// This serves two purposes:
// - To prompt the user when they're first opening the app and don't have any API keys set
// - To serve as the window through which the user changes their settings
const UserConfigModal = (props: {
  visible: boolean,
  oldConfig: UserConfig | null,
  sendMessage: (message: ArrakisRequest) => void,
  setSelectedModal: (modal: Modal) => void,
  setModel: (model: API) => void,
  setUserConfig: (newConfig: UserConfig) => void,
}) => {
  const [apiKeys, setApiKeys] = useState<ApiKeys>({
    openai: props.oldConfig ? props.oldConfig.apiKeys.openai : '',
    anthropic: props.oldConfig ? props.oldConfig.apiKeys.anthropic : '',
    gemini: props.oldConfig ? props.oldConfig.apiKeys.gemini : '',
    groq: props.oldConfig ? props.oldConfig.apiKeys.groq : '',
    grok: props.oldConfig ? props.oldConfig.apiKeys.grok : ''
  });

  useEffect(() => {
    setApiKeys({
      openai: props.oldConfig ? props.oldConfig.apiKeys.openai : '',
      anthropic: props.oldConfig ? props.oldConfig.apiKeys.anthropic : '',
      gemini: props.oldConfig ? props.oldConfig.apiKeys.gemini : '',
      groq: props.oldConfig ? props.oldConfig.apiKeys.groq : '',
      grok: props.oldConfig ? props.oldConfig.apiKeys.grok : ''
    });
  }, [props.oldConfig]);

  const handleInputChange = (provider: string) => (e: any) => {
    setApiKeys(prev => ({
      ...prev,
      [provider]: e.target.value
    }));
  };

  const handleSubmit = () => {
    const newConfig = UserConfigRequestSchema.parse({
      write: true,
      apiKeys,
      systemPrompt: props.oldConfig ? props.oldConfig.systemPrompt : '',
    });

    props.sendMessage({
      method: 'Config',
      payload: newConfig,
    } satisfies ArrakisRequest);

    props.setSelectedModal(null);

    const availableModels = filterAvailableModels(newConfig);
    props.setModel(availableModels[0] as any);
    props.setUserConfig(newConfig);
  };

  return (
    <div style={{
      position: 'fixed',
      backgroundColor: '#FDFEFE',
      minWidth: '480px',
      width: '25vw',
      height: '45vh',
      top: '50%',
      left: '50%',
      transform: 'translate(-50%, -50%)',
      overflow: 'hidden auto',
      borderRadius: '1rem',
      display: 'flex',
      flexDirection: 'column',
      justifyContent: 'center',
      padding: '24px',
    }}>
      <div style={{
        userSelect: 'none'
      }}>
        To work with William, you'll need to set at least one of the following API Keys:
      </div>

      {Object.entries({
        'OpenAI': 'openai',
        'Anthropic': 'anthropic',
        'Gemini': 'gemini',
        'Groq': 'groq'
      }).map(([label, key]) => (
        <div key={key} style={{
          display: 'flex',
          marginTop: '1rem',
          width: '100%',
          gap: '16px'
        }}>
          <span style={{
            width: '96px',
            display: 'flex',
            alignItems: 'center'
          }}>{label}</span>
          <input
            type="text"
            placeholder="sk-..."
            value={apiKeys[key as (keyof typeof apiKeys)]}
            onChange={handleInputChange(key)}
            style={{
              flex: 1,
              padding: '4px 8px',
              border: '1px solid #ccc',
              borderRadius: '4px'
            }}
          />
        </div>
      ))}

      <button
        onClick={handleSubmit}
        style={{
          width: 'fit-content',
          padding: '0 0.5rem',
          marginTop: '1rem',
          alignSelf: 'flex-end'
        }}
        disabled={!Object.values(apiKeys).some(key => key.trim().length > 0)}
      >
        Save
      </button>
    </div >
  );
};

function getAvailableModel(userConfig: UserConfig | null) {
  const availableModels = filterAvailableModels(userConfig);
  if (availableModels.length > 0) {
    // TODO: how do we convince the type system that `filterAvailableModels` returns bonafide API structs
    return availableModels[0] as any;
  } else {
    return { provider: 'openai', model: 'gpt-4o', label: 'GPT', };
  }
}

// The little dot in the top left
// Will tell the status of the backend connection + any errors when hovered
function ConnectionStatus(props: { error: Error | null, connectionStatus: string }) {
  // Array of [x, y]
  const [mousePosition, setMousePosition] = useState<number[]>([0, 0]);
  const [mouseIn, setMouseIn] = useState<boolean>(false);

  const padding = 5;

  return (
    <>
      <div
        style={{
          pointerEvents: 'none',
          position: 'fixed',
          left: `${mousePosition[0]}px`,
          top: `${mousePosition[1]}px`,
          transition: 'opacity 0.5s',
          opacity: mouseIn ? 1 : 0,
          fontSize: '14px',
          backgroundColor: '#F8F9F9',
          borderRadius: '5px',
          padding: `${padding}px`,
          color: props.error ? '#F44336' : '#4CAF50',
        }}
      >{props.error ? props.error.toString() : (props.connectionStatus === 'connected' ? 'Connected!' : 'Disconnected')}</div>
      <div
        style={{
          width: '36px',
          height: '100%',
        }}
        onMouseEnter={() => setMouseIn(true)}
        onMouseLeave={() => setMouseIn(false)}
        onMouseMove={(e: React.MouseEvent<HTMLDivElement>) => setMousePosition([e.clientX + padding, e.clientY + padding])}
      >
        <div
          style={{
            backgroundColor: props.error !== null || props.connectionStatus === 'disconnected' ? '#F44336' : '#4CAF50',
            userSelect: 'none',
            width: '6px',
            height: '6px',
            borderRadius: '50%',
            alignSelf: 'center',
            position: 'relative',
            left: '50%',
            top: '50%',
            transform: 'translate(-50%, -50%)',
          }}
        />
      </div>
    </>
  );
}

const ConversationHistoryElement = (props: {
  name: string,
  id: number | null,
  // Callback to load the conversation to the chat when this element is selected
  getLoadConversationCallback: any,
  // Our hook into the existing websocket connection
  sendMessage: any,
}) => {
  const [mousePosition, setMousePosition] = useState<number[]>([0, 0]);
  const [mouseIn, setMouseIn] = useState<boolean>(false);
  const [targetText, setTargetText] = useState<string>('');
  const [displayedText, setDisplayedText] = useState<string>('');

  // Use refs for intervalId and currentIndex
  const intervalIdRef = useRef<number | null>(null);
  const currentIndexRef = useRef<number>(0);

  useEffect(() => {
    // Clear any existing interval before starting a new one
    if (intervalIdRef.current !== null) {
      clearInterval(intervalIdRef.current);
    }

    // Reset current index and displayed text when targetText changes
    currentIndexRef.current = 0;
    setDisplayedText(targetText ? targetText[0] : '');

    const updateText = () => {
      if (currentIndexRef.current >= Math.min(100, targetText.length - 1)) {
        // Clear the interval when done
        if (intervalIdRef.current !== null) {
          clearInterval(intervalIdRef.current);
          intervalIdRef.current = null;
        }

        if (targetText.length > 100) {
          setDisplayedText((prev) => prev + '...');
        }

        return;
      }

      setDisplayedText((prev) => prev + targetText[currentIndexRef.current]);
      currentIndexRef.current++;
    };

    // Set up the interval
    intervalIdRef.current = window.setInterval(updateText, 10);

    // Clean up on unmount or when dependencies change
    return () => {
      if (intervalIdRef.current !== null) {
        clearInterval(intervalIdRef.current);
        intervalIdRef.current = null;
      }
    };
  }, [targetText, mouseIn]);

  const padding = 5;

  return (
    <>
      <div
        className="historyButton historyButtonModal"
        style={{
          pointerEvents: 'none',
          position: 'fixed',
          left: `${mousePosition[0]}px`,
          top: `${mousePosition[1]}px`,
          transition: 'opacity 0.5s',
          opacity: mouseIn ? 1 : 0,
          fontSize: '14px',
          backgroundColor: '#F8F9F9',
          borderRadius: '5px',
          padding: `${padding}px`,
          maxWidth: '234px',
        }}
      >{displayedText}</div>
      <div
        className="historyButton"
        style={{
          width: '60%',
          height: 'fit-content',
          cursor: 'pointer',
          userSelect: 'none',
          borderRadius: '0.5rem',
          textWrap: 'pretty',
          marginLeft: '16px',
        }}
        onMouseEnter={() => {
          setMouseIn(true);
          props.sendMessage(
            ArrakisRequestSchema.parse({
              method: 'Preview',
              payload: PreviewRequestSchema.parse({
                conversationId: props.id!,
                content: '',
              })
            }),
            (response: ArrakisResponse) => {
              const payload = PreviewResponseSchema.parse(response.payload);
              setTargetText(payload.content);
              setDisplayedText('');
            });
        }}
        onMouseLeave={() => {
          setMouseIn(false);
          setTargetText('');
          setDisplayedText('');
        }}
        onMouseMove={(e: React.MouseEvent<HTMLDivElement>) => setMousePosition([e.pageX + padding, e.pageY + padding])}
      >
        <div onClick={props.getLoadConversationCallback(props.id!)}>
          {formatTitle(props.name)}
        </div>
      </div>
    </>
  );
};

function MainPage() {
  const {
    connectionStatus,
    setUserConfig,
    userConfig,
    conversations,
    loadedConversation,
    setLoadedConversation,
    sendMessage,
    error,
  } = useWebSocket({
    url: 'ws://localhost:9001',
    retryInterval: 5000,
    maxRetries: 0
  });

  // These two are used to shove everything up and keep things above the actual chat box
  // at least, when the chat is at the bottom. The chat input will cover things otherwise.
  const inputContainerRef = useRef<HTMLDivElement | null>(null);
  // The unit of this is pixels
  const [inputContainerHeight, setInputContainerHeight] = useState<number>(0);

  // Triggered/set when a modal is selected from the menu buttons on the left side of the screen
  const [selectedModal, setSelectedModal] = useState<Modal | null>(null);

  // TODO: deprecated--this isn't being used anymore
  const [mouseInChat, setMouseInChat] = useState<boolean>(false);

  // This represents the model to be used to generate the next message in the conversation
  // Chosen through the dropdown (jump up?) menu in the bottom left
  const [model, setModel] = useState<API>(getAvailableModel(userConfig));

  // The conversation title card in the top left
  // TODO: this is a little unstable and needs debugging when conversation names are changing around
  const titleDefault = () => ({ title: '', index: 0 });
  const [displayedTitle, setDisplayedTitle] = useState<{ title: string; index: number; }>(titleDefault());

  // This is really only used to scroll the chat down to the bottom when a message is being streamed
  const messagesRef = useRef() as React.MutableRefObject<HTMLDivElement>;

  // To be honest I'm not really sure why this is here
  // The UI doesn't work around the chat input correctly without it though
  useEffect(() => {
    if (inputContainerRef.current) {
      const rect = inputContainerRef.current.getBoundingClientRect();
      setInputContainerHeight(_ => rect.height);

      const container = messagesRef.current;
      if (container) {
        // Check if the scrollbar is at the bottom
        const isScrolledToBottom =
          container.scrollHeight - container.scrollTop <= container.clientHeight + 18; // ???

        // If it is, scroll to the new bottom
        if (isScrolledToBottom) {
          container.scrollTop = container.scrollHeight;
        }
      }
    }
  }, [inputContainerHeight]);

  // Set the current model to one that's valid given the set API keys
  useEffect(() => {
    setModel(getAvailableModel(userConfig));
  }, [userConfig]);

  // Initial fetch of user's stored settings
  //
  // TODO: there should be a refactor of `sendMessage` to accept a callback for processing the response
  useEffect(() => {
    if (connectionStatus === 'connected') {
      sendMessage({
        method: 'Config',
        payload: {
          write: false,
          systemPrompt: '',
          apiKeys: {
            openai: '',
            grok: '',
            groq: '',
            gemini: '',
            anthropic: '',
          }
        } satisfies UserConfigRequest
      } satisfies ArrakisRequest);
    }
  }, [connectionStatus]);

  const needsOnboarding = (userConfig: UserConfig | null) => {
    return (userConfig &&
      !(userConfig.apiKeys.openai ||
        userConfig.apiKeys.groq ||
        userConfig.apiKeys.anthropic ||
        userConfig.apiKeys.grok ||
        userConfig.apiKeys.gemini));
  };

  // Once we receive the user settings from the backend,
  // we do a quick check to see if they've set their API keys
  //
  // Not having them set results in an onboarding modal to do so
  useEffect(() => {
    if (needsOnboarding(userConfig)) {
      setSelectedModal('config');
    }
  }, [userConfig]);

  // Start a new conversation
  const resetConversation = () => {
    setSelectedModal(null);
    setLoadedConversation(conversationDefault());
    setDisplayedTitle(titleDefault());
  };

  // Setup event listeners for typing anywhere -> focusing the input
  useEffect(() => {
    const handleKeyPress = (event: any) => {
      // We don't want to mess with things if the user is digging around outside the chat interface
      if (selectedModal !== null) {
        return;
      }

      // Checking hotkeys
      if (event.ctrlKey) {
        if (event.key === 'h') {
          setSelectedModal('search');
        } else if (event.key === 'n') {
          resetConversation();
        }
      }

      if (event.key === 'Escape') {
        setSelectedModal(null);
      }

      // List of keys that shouldn't trigger input focus
      const systemKeys = [
        8,   // Backspace
        9,   // Tab
        18,  // Alt
        20,  // Caps Lock
        27,  // Escape
        33,  // Page Up
        34,  // Page Down
        35,  // End
        36,  // Home
        37,  // Left Arrow
        38,  // Up Arrow
        39,  // Right Arrow
        40,  // Down Arrow
        45,  // Insert
        46,  // Delete
        112, // F1
        113, // F2
        114, // F3
        115, // F4
        116, // F5
        117, // F6
        118, // F7
        119, // F8
        120, // F9
        121, // F10
        122, // F11
        123, // F12
      ];

      // Don't trigger on system keys or modifier combinations
      if (
        systemKeys.includes(event.keyCode) ||
        (event.ctrlKey && (event.key != 'v' || event.key == 'c')) ||  // copy + paste
        (event.metaKey && (event.key != 'v' || event.key == 'c')) ||  // copy + paste
        event.altKey
      ) {
        return;
      }

      // Keep a newline from being input when a message is trying to be sent
      if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
      }

      // Finally at the point where we want to actually focus the input
      (document.getElementById('chatInput') as HTMLInputElement).focus();
    }

    document.addEventListener('keydown', handleKeyPress);

    return () => {
      document.removeEventListener('keydown', handleKeyPress);
    };
  }, [selectedModal, mouseInChat]);

  function isGuid(str: string): boolean {
    const guidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    return guidRegex.test(str);
  }

  // Scroll the conversation to the bottom while streaming the messages
  useEffect(() => {
    if (messagesRef.current) {
      messagesRef.current.scrollTo({
        top: messagesRef.current.scrollHeight,
        behavior: 'auto'
      });
    }

    // Loading the conversation name
    // This incrementally loads each character of the conversation name
    // to mimic a typing motion
    let intervalId: any = null;
    if (!isGuid(loadedConversation.name)) {
      intervalId = setInterval(() => {
        const conversationName = loadedConversation.name;

        if (displayedTitle.index < conversationName.length) {
          setDisplayedTitle(prev => {
            if (prev.index < conversationName.length) {
              return { title: prev.title + conversationName[prev.index], index: prev.index + 1 };
            } else {
              return prev;
            }
          });
        } else {
          clearInterval(intervalId);
        }
      }, 50);
    }

    return () => {
      if (intervalId) {
        clearInterval(intervalId);
      }
    };
  }, [loadedConversation]);

  // This is the chat message submit
  // Takes care of the logic of:
  // - Sending the conversation through William's backend
  // - Updating the UI with the new message
  const handleChatInput = (e: any) => {
    const inputElement = document.getElementById('chatInput') as HTMLDivElement;
    if (e.key === 'Enter') {
      // This is a message submission, not a newline
      if (!e.shiftKey) {
        e.preventDefault();
        const data = inputElement.innerText;

        // We don't want to allow empty message submissions
        if (data.length === 0) {
          return;
        }

        // Update the local conversation message array
        // Current logic is to send an empty placeholder message for the Assistant
        // This is what gets populated by the response
        const messages = loadedConversation.messages;
        const newMessages = [
          ...messages,
          {
            id: messages.length > 0 ? messages[messages.length - 1].id! + 1 : null,
            content: data,
            message_type: 'User',
            api: model,
            system_prompt: '',
            sequence: messages.length
          } satisfies Message,
          {
            id: messages.length > 0 ? messages[messages.length - 1].id! + 2 : null,
            content: '',
            message_type: 'Assistant',
            api: model,
            system_prompt: '',
            sequence: messages.length + 1
          } satisfies Message,
        ];

        const newConversation = {
          ...loadedConversation,
          messages: newMessages,
        };

        // State update
        setLoadedConversation(newConversation);

        // Send the updated conversation to the backend
        sendMessage({
          method: 'Completion',
          payload: newConversation,
        } satisfies ArrakisRequest);

        // Reset the chat input
        inputElement.innerHTML = '';
      }

      // The newline case is handled implicitly here
    }
  };

  // Ping to see if we're connected to the backend
  // TODO: I don't think this should really be here
  //       The disconnection from the backend has some pretty immediate feedback
  useEffect(() => {
    const pingInterval = setInterval(() => {
      sendMessage(ArrakisRequestSchema.parse({
        method: 'Ping',
        payload: {
          body: 'ping',
        } satisfies PingRequest,
      }));
    }, 5000);

    // Clean up interval when component unmounts
    return () => clearInterval(pingInterval);
  }, [sendMessage]);

  // Whenever a modal is selected, get a list of the saved conversations from the backend
  // TODO: this could probably be cleaned up to be only when the History modal is loaded
  useEffect(() => {
    sendMessage({
      method: 'ConversationList',
    } satisfies ArrakisRequest);
  }, [selectedModal]);

  const getConversationCallback = (id: number) => {
    return () => {
      setDisplayedTitle(titleDefault());
      close();
      sendMessage({
        method: 'Load',
        payload: {
          id,
        } satisfies LoadRequest
      } satisfies ArrakisRequest);
    };
  };

  // Decide which modal to build + return based on the currently selected modal
  const buildHistoryModal = () => {
    const [searchInput, setSearchInput] = useState<string>('');

    const close = () => {
      setSelectedModal(null)
      setSearchInput('');
    };

    // Setup event listeners for typing anywhere -> focusing the input
    // This is more or less a copy of the listener for the main chat input
    useEffect(() => {
      const handleKeyPress = (event: any) => {
        // We don't want to mess with things if the user is digging around outside the chat interface
        if (selectedModal !== 'search') {
          return;
        }

        if (event.key === 'Escape') {
          setSelectedModal(null);
        }

        // List of keys that shouldn't trigger input focus
        const systemKeys = [
          8,   // Backspace
          9,   // Tab
          18,  // Alt
          20,  // Caps Lock
          27,  // Escape
          33,  // Page Up
          34,  // Page Down
          35,  // End
          36,  // Home
          37,  // Left Arrow
          38,  // Up Arrow
          39,  // Right Arrow
          40,  // Down Arrow
          45,  // Insert
          46,  // Delete
          112, // F1
          113, // F2
          114, // F3
          115, // F4
          116, // F5
          117, // F6
          118, // F7
          119, // F8
          120, // F9
          121, // F10
          122, // F11
          123, // F12
        ];

        // Don't trigger on system keys or modifier combinations
        if (
          systemKeys.includes(event.keyCode) ||
          (event.ctrlKey && (event.key != 'v' || event.key == 'c')) ||  // copy + paste
          (event.metaKey && (event.key != 'v' || event.key == 'c')) ||  // copy + paste
          event.altKey
        ) {
          return;
        }

        // Finally at the point where we want to actually focus the input
        (document.getElementById('search') as HTMLInputElement).focus();
      }

      document.addEventListener('keydown', handleKeyPress);

      return () => {
        document.removeEventListener('keydown', handleKeyPress);
      };
    }, [selectedModal, mouseInChat]);

    const headerHeight = 108;

    // Build the modal HTML with the listed conversations
    return (
      <div
        className="scrollbar"
        style={{
          transition: 'all 0.3s',
          opacity: selectedModal === 'search' ? 0.975 : 0,
          position: 'fixed',
          backgroundColor: 'transparent',
          width: '100vw',
          height: '100vh',
          top: '50%',
          left: '50%',
          transform: 'translate(-50%, -50%)',
          overflow: 'hidden auto',
          borderRadius: '1rem',
          zIndex: 750,
        }}
        onClick={close}
      >
        <div
          style={{
            display: 'flex',
            position: 'fixed',
            width: '100%',
            height: 'calc(24px + 1rem + 0.5rem)',
          }}>
          <div
            className="buttonHoverLight"
            style={{
              cursor: 'pointer',
              padding: '0.25rem',
              margin: '0.5rem', // 0.5rem to line up with the combination of history selection container and the selections themselves
              width: '24px',
              height: '24px',
              borderRadius: '0.5rem',
            }}
            onClick={close}
          >
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
              <g stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <line x1="6" y1="6" x2="18" y2="18" />
                <line x1="18" y1="6" x2="6" y2="18" />
              </g>
            </svg>
          </div>
          <div
            style={{
              position: 'absolute',
              left: '50%',
              top: '0.75rem',
              transform: 'translateX(-50%)',
            }}>Chat History</div>
        </div>
        { /* Filler to push the search enough down below the header */}
        <div
          style={{
            height: 'calc(24px + 1rem + 0.5rem)',
            backgroundColor: 'transparent',
          }}
        />
        { /* Height here chosen to fill out the rest of the header height, totalling to `${headerHeight}`px */}
        <div
          style={{
            height: `calc(${headerHeight}px - (24px + 1rem + 0.5rem))`,
            width: 'fit-content',
            margin: 'auto',
          }}
          onClick={(e: any) => e.stopPropagation()}
        >
          <input
            type="text"
            placeholder="Search"
            id="search"
            value={searchInput}
            onChange={(e: any) => setSearchInput(e.target.value)}
            style={{
              outline: 0,
              border: '1px solid #EDEFEF',
              boxShadow: '0 2px 4px rgba(0, 0, 0, 0.1)',
              backgroundColor: '#FFFFFF',
              padding: '0.5rem',
              height: '16px',
              borderRadius: '0.5rem',
              fontSize: '14px',
              width: '45vw',
            }}
          />
        </div>
        <div style={{
          justifyContent: 'center',
          display: 'flex',
          flexWrap: 'wrap',
        }}>
          {conversations
            .filter(c => searchInput === '' ||
              c.name.toLowerCase().includes(searchInput.toLowerCase()))
            .map(c => (
              <ConversationHistoryElement
                key={c.id}
                name={c.name}
                id={c.id}
                getLoadConversationCallback={getConversationCallback}
                sendMessage={sendMessage} />
            ))}
        </div>
      </div>

    );
  };

  // Function for parsing a string + adding Latex math delimiters for Markdown-It-Katex to properly render
  const addMathDelimiters = (input: string) => {
    const openChars = ['\\(', '\\['];
    const closeChars = ['\\)', '\\]'];

    let result = '';
    let currentIndex = 0;

    while (currentIndex < input.length) {
      // Find next opening character
      let foundOpenChar = false;
      let openCharIndex = -1;
      let matchedCloseChar = '';

      for (let i = 0; i < openChars.length; i++) {
        const index = input.indexOf(openChars[i], currentIndex);
        if (index !== -1 && (openCharIndex === -1 || index < openCharIndex)) {
          openCharIndex = index;
          matchedCloseChar = closeChars[i];
          foundOpenChar = true;
        }
      }

      if (!foundOpenChar) {
        // Add remaining text and break
        result += input.slice(currentIndex);
        break;
      }

      // Add text before the math content
      result += input.slice(currentIndex, openCharIndex);

      // Find closing character
      const closeCharIndex = input.indexOf(matchedCloseChar, openCharIndex + 2);

      // Extract and process math content
      if (closeCharIndex === -1) {
        // No closing character found
        const mathContent = input.slice(openCharIndex + 2);
        const hasNewline = mathContent.includes('\n');
        result += hasNewline ? '$$' + mathContent + '$$' : '$' + mathContent + '$';
        break;
      } else {
        // Closing character found
        const mathContent = input.slice(openCharIndex + 2, closeCharIndex).trim();
        const hasNewline = mathContent.includes('\n');
        result += hasNewline ? '$$' + mathContent + '$$' : '$' + mathContent + '$';
        currentIndex = closeCharIndex + 2;
      }
    }

    return result;
  };

  // TODO: what's the type here?
  //
  // Creates a blurred backdrop to be placed behind a modal
  // Covers the entire screen and assumes click priority over everything else behind the modal
  const buildModalBackdrop = (onClickCallback: any, triggerCondition: boolean) => {
    const blur = 16;
    return (
      <div style={{
        position: 'fixed',
        left: 0,
        top: 0,
        height: '100vh',
        width: '100vw',
        backgroundColor: '#0000000A',
        backdropFilter: triggerCondition ? `blur(${blur}px)` : 'blur(0px)',
        WebkitBackdropFilter: triggerCondition ? `blur(${blur}px)` : 'blur(0px)',
        transition: 'all 0.3s',
        opacity: triggerCondition ? 1 : 0,
        zIndex: 500,
      }} onClick={onClickCallback} />
    );
  };

  // Main window component
  return (
    <div ref={messagesRef} className="scrollbar" onMouseEnter={() => setMouseInChat(true)} onMouseLeave={() => setMouseInChat(false)} style={{
      height: '100vh',
      display: 'flex',
      flexDirection: 'column',
      width: '100vw',
      overflowY: 'scroll',
      backgroundColor: '#F8F9F9',
    }}>
      { /* Header */}
      <div style={{
        position: 'fixed',
        height: '2.5rem',
        width: '100%',
        display: 'flex',
        backgroundColor: '#F8F9F9',
        boxShadow: '0 2px 4px rgba(0, 0, 0, 0.1)',
        paddingLeft: '0.5rem',
        zIndex: 1,
      }}>
        { /* Element to determine whether the frontend has an established websocket connection with the backend */}
        <ConnectionStatus error={error} connectionStatus={connectionStatus} />

        { /* Create a new conversation and clear the current conversation history */}
        <div className="buttonHoverLight" onClick={resetConversation} style={menuButtonStyle}>New</div>

        { /* View + give the option to load saved conversations */}
        <div
          className="buttonHoverLight"
          onClick={() => setSelectedModal('search')}
          style={menuButtonStyle}>History</div>

        { /* Updating user configuration (as of writing, just API keys */}
        <div
          className="buttonHoverLight"
          onClick={() => setSelectedModal('config')}
          style={menuButtonStyle}>Settings</div>

        { /* Dropdown for the user to change which LLM provider + backend they're using for the next message/fork */}
        <ModelDropdown userConfig={userConfig} model={model.model} modelCallback={setModel} />

        { /* Display element for the selected modal, if any */}
        <div style={{
          pointerEvents: selectedModal === 'search' ? 'auto' : 'none',
          transition: 'all 0.3s',
        }}>
          {buildModalBackdrop(() => setSelectedModal(null), selectedModal === 'search')}
          {buildHistoryModal()}
        </div>
      </div>

      {
        /*
         * This is primarily an onboarding component to check if the user has their API keys set
         * Really, the app is useless without API keys so we're blocking them from moving forward 
         * until they're configured.
         *
         * I think this same component will be used for manual configuration triggers later on
         *
         * TODO: this doesn't trigger as a separate component???
         */
      }
      <div
        style={{
          pointerEvents: selectedModal === 'config' ? 'auto' : 'none',
        }}>
        {buildModalBackdrop(!needsOnboarding(userConfig) ? () => setSelectedModal(null) : () => { }, selectedModal === 'config')}
        <div
          style={{
            transition: 'all 0.3s',
            opacity: selectedModal === 'config' ? 1 : 0,
            zIndex: 1000,
            position: 'fixed',
          }}>
          <UserConfigModal
            visible={selectedModal === 'config'}
            oldConfig={userConfig}
            sendMessage={sendMessage}
            setSelectedModal={setSelectedModal}
            setModel={setModel}
            setUserConfig={setUserConfig}
          />
        </div>
      </div>

      {
        /*
         * This is the main wrapper around the chat input that places it in the center of the screen
         * TODO: revisit how much of this is actually needed
         */
      }
      <div
        ref={inputContainerRef}
        style={{
          position: 'fixed',
          left: 'calc(50%)',
          transform: 'translateX(-50%)',
          bottom: '16px',
          // -2rem for margins + padding on the sides
          width: 'calc(100% - 3rem)',
          // 45% of 1920
          maxWidth: '864px',
          minHeight: '16px',
          padding: '12px',
          border: '1px solid #EDEFEF',
          backgroundColor: '#EDEFEF',
          boxShadow: '0 2px 4px rgba(0, 0, 0, 0.1)',
          borderRadius: '0.5rem',
          fontSize: '14px',
          overflow: 'hidden',
          display: 'flex',
          zIndex: selectedModal !== null ? 0 : 3,
        }}
      >
        <div style={{
          maxHeight: '25vh',
          overflow: 'auto',
          height: '100%',
          width: '100%',
          display: 'flex',
          flexDirection: 'column',
        }}>
          <div
            contentEditable={true}
            id="chatInput"
            onKeyDown={handleChatInput}
            onKeyUp={() => {
              if (inputContainerRef.current) {
                setInputContainerHeight(_ => inputContainerRef.current!.getBoundingClientRect().height);
              }
            }}
            style={{
              height: '100%',
              width: '100%',
              border: 0,
              outline: 0,
              resize: 'none',
              alignSelf: 'center',
              backgroundColor: 'transparent',
            }}
          />
          <div style={{
            display: 'flex',
          }}>

          </div>
        </div>
      </div>

      { /* Conversation/message list contents */}
      <div style={{
        position: 'relative',
        maxWidth: '768px',
        minWidth: '40vw',
        left: '50%',
        transform: 'translate(-50%)',
        marginLeft: '20px',
        marginRight: '20px',
        top: 'calc(2.5rem + 1px)',
        flex: 1,
        // distance of input container from bottom of screen +
        // (2x from position relative) height of the input container +
        // (2x from position relative) padding of the input container +
        // arbitrary margin to keep things above the input container
        marginBottom: `calc(16px + ${inputContainerHeight}px + 24px + 24px)`
      }}>
        {loadedConversation.messages.map((m, i) => {
          // Lot of preprocessing here to properly render the messages into markdown into react components
          // particularly, HTML characters need properly escaped in order to be processed correctly
          // CSS styles also need to be changed--the generated HTML from markdown-it isn't conducive to React inline styling
          // i.e., it's native to HTML rather than camelCase/JS-ified

          const toPattern = new RegExp(
            Object.keys(escapeToHTML)
              .map(key => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
              .join('|'),
            'g'
          );

          // Escaping the incoming messages to be HTML friendly
          // Particularly, if a message contains HTML it can message with the markdown-parsed output
          // The escaping occurs here to keep from disambiguiation issues later in the pipeline
          let content = m.content.replace(toPattern, function(match) {
            return escapeToHTML[match];
          });

          // Markdown to HTML conversion
          content = addMathDelimiters(content);
          content = md.render(content) as string;

          const reactElements = htmlToReactElements(content);

          // Processing each react element to clean the contained text
          // and fix up the CSS styles to be camelCase
          function modifyElements(element: any): ReactElementOrText {
            if (typeof element === 'string') {
              let c = element as string;
              const fromPattern = new RegExp(
                Object.keys(escapeFromHTML)
                  .map(key => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
                  .join('|'),
                'g'
              );

              return c.replace(fromPattern, function(match) {
                return escapeFromHTML[match];
              })
            }

            const props = element.props as React.PropsWithChildren<{ [key: string]: any }>;

            const input = props.style;

            if (input) {
              if (typeof input !== "string") return null;

              const styleObject: { [key: string]: any } = {};
              const styleEntries = input.split(";").filter(Boolean);

              for (const entry of styleEntries) {
                const [property, value] = entry.split(":").map((s) => s.trim());
                if (property && value) {
                  // Convert CSS property to camelCase for React style
                  const camelCaseProperty = property.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
                  styleObject[camelCaseProperty] = value;
                }
              }

              return React.cloneElement(element, {
                style: styleObject,
                children: React.Children.map(props.children, (child) =>
                  React.isValidElement(child) ? modifyElements(child) : child
                ),
              });
            }

            if (props.children) {
              return React.createElement(
                element.type,
                element.props,
                React.Children.map(props.children, modifyElements)
              );
            }

            return element;
          };

          const unescapedElements = reactElements.map(modifyElements);

          const isUser = m.message_type === 'User';

          // Actually build the component which holds the chat history + all the contained messages
          return (
            <>
              <div style={{
                backgroundColor: isUser ? '#E8E9E9' : '',
                borderRadius: '0.5rem',
                margin: '2rem 0.25rem 1.5rem 0.25rem',
                padding: '0.01rem 0',
                width: isUser ? 'fit-content' : '',
                marginLeft: isUser ? 'auto' : '',
                position: 'relative',
                fontSize: '14px',
              }}>
                {
                  /* The actual message elements */
                  i < loadedConversation.messages.length - 1 || unescapedElements.length > 0 ? unescapedElements : (
                    <div
                      className={`text-placeholder`}
                      aria-live="polite"
                    >
                      {'Thinking...'}
                    </div>
                  )
                }
                {isUser ? '' : (
                  <p className="messageOptions" style={{
                    position: 'absolute',
                    transform: 'translateY(calc(-100% + 0.5rem))',
                    userSelect: 'none',
                    cursor: 'pointer',
                    display: 'flex',
                  }}>
                    <div>•</div>
                    <div style={{
                      width: 'fit-content',
                      overflow: 'hidden',
                    }}>
                      <div className="messageOptionsRow">
                        <div style={{
                          padding: '0 0.5rem',
                        }} onClick={() => {
                          // Regeneration option for a given message
                          // This forks the existing conversation and saves the new conversation as a new entry in the listing
                          // All conversation up to the regenerated message is kept, all conversation history after is left behind in the old conversation

                          sendMessage(ArrakisRequestSchema.parse({
                            method: 'Fork',
                            payload: ForkRequestSchema.parse({
                              conversationId: loadedConversation.id,
                              sequence: m.sequence
                            })
                          }));

                          const conversation = {
                            ...loadedConversation,
                            messages: loadedConversation.messages.slice(0, m.sequence + 1),
                          };

                          let last = conversation.messages[conversation.messages.length - 1];

                          // Cleaning message metadata for the new conversation entry
                          last.content = '';
                          last.id = null;
                          last.message_type = 'Assistant';
                          last.system_prompt = userConfig ? userConfig.systemPrompt : '';
                          last.api = model;

                          conversation.messages[conversation.messages.length - 1] = last;

                          setLoadedConversation(conversation);
                        }}>Regenerate</div>
                      </div>
                    </div>
                  </p>
                )}
              </div>
            </>
          );
        })}
      </div>
    </div >
  );
}

root.render(
  <MainPage />
);
