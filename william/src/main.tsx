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
  highlight: function (str, lang) {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return hljs.highlight(str, { language: lang }).value;
      } catch (__) { }
    }
    return ''; // use external default escaping
  }
}).use(markdownItKatex);

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
  systemPrompt: string;
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

const SystemPromptRequestSchema = z.object({
  content: z.string(),
  write: z.boolean(),
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

const CompletionResponseSchema = z.object({
  stream: z.boolean(),
  delta: z.string(),
  name: z.string(),
  conversationId: z.number(),
  requestId: z.number(),
  responseId: z.number(),
});

const SystemPromptResponseSchema = SystemPromptRequestSchema;

const PingResponseSchema = PingRequestSchema;

const ConversationListResponseSchema = z.object({
  conversations: z.array(ConversationSchema),
});

const ArrakisRequestSchema = z.discriminatedUnion("method", [
  z.object({
    method: z.literal("ConversationList"),
  }),
  z.object({
    method: z.literal("Ping"),
    payload: PingRequestSchema,
  }),
  z.object({
    method: z.literal("Completion"),
    payload: CompletionRequestSchema,
  }),
  z.object({
    method: z.literal("Load"),
    payload: LoadRequestSchema,
  }),
  z.object({
    method: z.literal("SystemPrompt"),
    payload: SystemPromptRequestSchema,
  }),
  z.object({
    method: z.literal("Fork"),
    payload: ForkRequestSchema,
  }),
]);

const ArrakisResponseSchema = z.discriminatedUnion("method", [
  z.object({
    method: z.literal("ConversationList"),
    payload: ConversationListResponseSchema,
  }),
  z.object({
    method: z.literal("Ping"),
    payload: PingResponseSchema,
  }),
  z.object({
    method: z.literal("Completion"),
    payload: CompletionResponseSchema,
  }),
  z.object({
    method: z.literal("SystemPrompt"),
    payload: SystemPromptResponseSchema,
  }),
]);

type API = z.infer<typeof APISchema>;
type Message = z.infer<typeof MessageSchema>;
type Conversation = z.infer<typeof ConversationSchema>;
type SystemPromptRequest = z.infer<typeof SystemPromptRequestSchema>;
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
const useWebSocket = ({
  url,
  retryInterval = 5000,
  maxRetries = 0
}: WebSocketHookOptions): WebSocketHookReturn => {
  const [socket, setSocket] = useState<WebSocket | null>(null);
  const [systemPrompt, setSystemPrompt] = useState<string>('');
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [loadedConversation, setLoadedConversation] = useState<Conversation>(conversationDefault());
  const [connectionStatus, setConnectionStatus] = useState<'connected' | 'disconnected'>('disconnected');
  const [error, setError] = useState<Error | null>(null);
  const [retryCount, setRetryCount] = useState(0);

  const connect = useCallback(() => {
    try {
      const ws = new WebSocket(url);

      ws.onopen = () => {
        setConnectionStatus('connected');
        setError(null);
        setRetryCount(0);
      };

      // This is a sorry excuse for a REST-ish API
      // I feel like there's a much better way of structuring the "endpoints" supported by both the front + back ends
      ws.onmessage = (event) => {
        try {
          const response = JSON.parse(event.data) satisfies ArrakisResponse;
          if (response.payload.method === 'Completion') {
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
          } else if (response.payload.method === 'Ping' && connectionStatus !== 'connected') {
            setConnectionStatus('connected');
          } else if (response.payload.method === 'ConversationList') {
            const conversationList = ConversationListResponseSchema.parse(response.payload);
            setConversations(conversationList.conversations);
          } else if (response.payload.method === 'Load') {
            const conversation = ConversationSchema.parse(response.payload);
            setLoadedConversation(conversation);
          } else if (response.payload.method === 'SystemPrompt') {
            setSystemPrompt(response.payload.content);
          }
        } catch (error) {
          console.log(error);
        }
      };

      // TODO: this needs to be expanded
      ws.onerror = (_) => {
        setError(new Error('WebSocket error occurred'));
      };

      ws.onclose = () => {
        setConnectionStatus('disconnected');
        setSocket(null);

        if (retryCount < maxRetries) {
          setTimeout(() => {
            setRetryCount(prev => prev + 1);
            connect();
          }, retryInterval);
        }
      };

      setSocket(ws);
    } catch (err) {
      setError(err instanceof Error ? err : new Error('Failed to create WebSocket connection'));
    }
  }, [url, retryCount, maxRetries, retryInterval]);

  // Generic message sending function for the backend
  // Ideally, _all_ messages going to the backend will be an ArrakisRequest
  const sendMessage = useCallback((message: ArrakisRequest) => {
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(typeof message === 'string' ? message : JSON.stringify(message));
    } else {
      console.error('WebSocket is not connected');
    }
  }, [socket]);

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
    systemPrompt,
    conversations,
    loadedConversation,
    setLoadedConversation,
    sendMessage,
    connectionStatus,
    error
  };
};

// TODO: do something with this and scrap it please
interface Sizing {
  value: number;
  unit: string;
  toString(): string;
}

function createSizing(value: number, unit: string): Sizing {
  return {
    value, unit, toString: () => { return `${value}${unit}`; }
  };
}

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

// Generic dropdown for setting the current LLM backend, which updates the main app state through props.modelCallback
const ModelDropdown = (props: { model: string, modelCallback: Function }) => {
  const [isOpen, setIsOpen] = useState(false);
  const popupRef = useRef<HTMLDivElement | null>(null);
  const buttonRef = useRef<HTMLDivElement | null>(null);

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

    document.addEventListener('mousedown', handleClickOutside as any);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside as any);
    };
  }, []);

  return (
    <div
      ref={buttonRef}
      onClick={() => setIsOpen(!isOpen)}
      className="buttonHoverLight"
      style={{
        userSelect: 'none',
        cursor: 'default',
        alignSelf: 'center',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        textAlign: 'center',
        padding: '0 0.25rem',
        borderRadius: '0.5rem',
      }}>
      {props.model}

      {
        isOpen && (
          <div
            ref={popupRef}
            className="popup-content"
          >
            {
              [
                { model: "gpt-4o", provider: "openai" },
                { model: "gpt-4o-mini", provider: "openai" },
                { model: "o1-preview", provider: "openai" },
                { model: "o1-mini", provider: "openai" },
                { model: "llama3-70b-8192", provider: "groq" },
                { model: "claude-3-opus-20240229", provider: "anthropic" },
                { model: "claude-3-sonnet-20240229", provider: "anthropic" },
                { model: "claude-3-haiku-20240307", provider: "anthropic" },
                { model: "claude-3-5-sonnet-latest", provider: "anthropic" },
                { model: "claude-3-5-haiku-latest", provider: "anthropic" }
              ].map(m => (
                <div
                  onClick={() => props.modelCallback(m)}
                  className="buttonHover"
                  style={{
                    textWrap: 'nowrap',
                    padding: '0.5rem',
                  }}>
                  {m.model}
                </div>
              ))
            }
          </div>
        )
      }
    </div >
  );
};

type Modal = 'search' | 'none';

function MainPage() {
  const {
    connectionStatus,
    systemPrompt,
    conversations,
    loadedConversation,
    setLoadedConversation,
    sendMessage,
  } = useWebSocket({
    url: 'ws://localhost:9001',
    retryInterval: 5000,
    maxRetries: 0
  });

  // Triggered/set when a modal is selected from the menu buttons on the left side of the screen
  const [selectedModal, setSelectedModal] = useState<Modal | null>(null);

  // TODO: deprecated--this isn't being used anymore
  const [mouseInChat, setMouseInChat] = useState<boolean>(false);

  // This represents the model to be used to generate the next message in the conversation
  // Chosen through the dropdown (jump up?) menu in the bottom left
  const [model, setModel] = useState<API>({ provider: 'anthropic', model: 'claude-3-5-sonnet-latest' });

  // The conversation title card in the top left
  // TODO: this is a little unstable and needs debugging when conversation names are changing around
  const titleDefault = () => ({ title: '', index: 0 });
  const [displayedTitle, setDisplayedTitle] = useState<{ title: string; index: number; }>(titleDefault());

  // TODO: ???
  //       this is unbelievably stupid
  const [inputSizings, _] = useState({
    height: createSizing(0, 'px'),
    padding: createSizing(0.75, 'em'),
    margin: createSizing(1, 'em'),
  });

  // This is really only used to scroll the chat down to the bottom when a message is being streamed
  const messagesRef = useRef() as React.MutableRefObject<HTMLDivElement>;

  // TODO: This is remarkably terrible
  //       This is supposed to be a sort of GET request to get the system prompt
  //       but the nature and presentation of it needs cleaned up badly
  useEffect(() => {
    if (connectionStatus === 'connected') {
      sendMessage({
        method: 'SystemPrompt',
        payload: {
          write: false,
          content: '',
        } satisfies SystemPromptRequest
      } satisfies ArrakisRequest);

    }
  }, [connectionStatus]);

  useEffect(() => {
    const handleKeyPress = (event: any) => {
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

  useEffect(() => {
    // Scroll the conversation to the bottom while streaming the messages
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
  const sendPrompt = (e: any) => {
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

  // Decide which modal to build + return based on the currently selected modal
  const getModal = () => {
    if (selectedModal === 'search') {
      // Callback to load a given conversation based on its ID
      // Closes the modal, updates the title, and fetches the conversation from the backend
      const getConversationCallback = (id: number) => {
        return () => {
          setDisplayedTitle(titleDefault());
          setSelectedModal(null);
          sendMessage({
            method: 'Load',
            payload: {
              id,
            } satisfies LoadRequest
          } satisfies ArrakisRequest);
        };
      };

      // Build the modal HTML with the listed conversations
      return (
        <div style={{
          margin: '0.5rem',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}>
          {conversations.map(c => {
            return (
              <div className="buttonHoverLight" onClick={getConversationCallback(c.id!)} style={{
                padding: '0.5rem',
                cursor: 'pointer',
                userSelect: 'none',
                borderRadius: '0.5rem',
                textWrap: 'nowrap',
              }}>
                {formatTitle(c.name)}
              </div>
            );
          })}
        </div>
      );
    }
    // Show the system prompt textarea for reading/writing
    // TODO: make this feature actually work\
    //
    // else if (selectedModal === 'systemPrompt') {
    //   return (
    //     <div style={{
    //       margin: '0.5rem',
    //       display: 'flex',
    //       flexDirection: 'column',
    //     }}>
    //       <textarea id="promptInput" placeholder="You are a helpful assistant." style={{
    //         border: 0,
    //         position: 'relative',
    //         top: '1px',
    //         outline: 0,
    //         resize: 'none',
    //         height: '45vh',
    //         borderRadius: '0.5rem',
    //         fontSize: '16px',
    //         padding: '0.5rem',
    //         marginBottom: '0.5rem',
    //         textWrap: 'nowrap',
    //       }} onBlur={() => {
    //         // The system prompt is updated on the backend + stored to disk
    //         // _only_ when the user unfocuses the textarea
    //         // TODO: this needs changed--it's unintuitive
    //         sendMessage({
    //           method: 'SystemPrompt',
    //           payload: {
    //             write: true,
    //             content: (document.getElementById('promptInput')! as HTMLTextAreaElement).value,
    //           } satisfies SystemPromptRequest
    //         } satisfies ArrakisRequest);
    //       }}>{systemPrompt}</textarea>
    //     </div>
    //   );
    // }
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

  const menuButtonStyle: React.CSSProperties = {
    userSelect: 'none',
    cursor: 'default',
    alignSelf: 'center',
    display: 'flex',
    justifyContent: 'center',
    alignItems: 'center',
    textAlign: 'center',
    padding: '0 0.25rem',
    borderRadius: '0.5rem',
  };

  // Main window component
  return (
    <div ref={messagesRef} onMouseEnter={() => setMouseInChat(true)} onMouseLeave={() => setMouseInChat(false)} style={{
      height: '100vh',
      display: 'flex',
      flexDirection: 'column',
      width: '100vw',
      overflowY: 'auto',
      backgroundColor: '#F9F8F7',
    }}>
      { /* Header */ }
      <div style={{
        position: 'fixed',
        height: '2.5rem',
        width: '100%',
        display: 'flex',
        gap: '1.5rem',
        backgroundColor: '#FFFFFF',
        paddingLeft: '0.5rem',
        borderBottom: '1px solid #CFCFCF',
        boxShadow: '0 1px 3px rgba(0, 0, 0, 0.12)',
        zIndex: 1,
      }}>
        { /* Element to determine whether the frontend has an established websocket connection with the backend */ }
        <div style={{
          backgroundColor: connectionStatus === 'disconnected' ? '#F44336' : '#4CAF50',
          userSelect: 'none',
          width: '24px',
          height: '24px',
          borderRadius: '0.5rem',
          alignSelf: 'center',
        }} />

        { /* Create a new conversation and clear the current conversation history */ }
        <div className="buttonHoverLight" onClick={() => {
          setSelectedModal(null);
          setLoadedConversation(conversationDefault());
          setDisplayedTitle(titleDefault());
        }} style={menuButtonStyle}>New</div>

        { /* View + give the option to load saved conversations */ }
        <div className="buttonHoverLight" onClick={() => setSelectedModal(selectedModal !== 'search' ? 'search' : null)} style={menuButtonStyle}>History</div>

        <ModelDropdown model={model.model} modelCallback={setModel} />

        { /* Display element for the selected modal, if any */ }
        <div style={{
          pointerEvents: selectedModal ? 'auto' : 'none',
          transition: 'all 0.3s',
        }}>
          <div style={{
            position: 'fixed',
            left: 0,
            top: 0,
            height: '100vh',
            width: '100vw',
            backgroundColor: 'rgba(236, 240, 255, 0.08)',
            backdropFilter: selectedModal ? 'blur(2px)' : 'blur(0px)',
            WebkitBackdropFilter: selectedModal ? 'blur(2px)' : 'blur(0px)',
            transition: 'all 0.3s',
            opacity: selectedModal ? 1 : 0,
          }} onClick={() => setSelectedModal(null)} />
          <div style={{
            backdropFilter: selectedModal ? 'blur(2px)' : 'blur(0px)',
            WebkitBackdropFilter: selectedModal ? 'blur(2px)' : 'blur(0px)',
            transition: 'all 0.3s',
            opacity: selectedModal ? 1 : 0,
            position: 'fixed',
            backgroundColor: '#F9F8F7',
            width: '55vw',
            height: '45vh',
            top: '50%',
            left: '50%',
            transform: 'translate(-50%, -50%)',
            overflow: 'hidden auto',
            borderRadius: '1rem',
          }}>
            {getModal()}
          </div>
        </div>
      </div>

      {
        /*
         * This is the main wrapper around the chat input that places it in the center of the screen
         * TODO: revisit how much of this is actually needed
         */
      }
      <div
        style={{
          position: 'fixed',
          left: 'calc(50%)',
          transform: 'translateX(-50%)',
          bottom: '1rem',
          // -2rem for margins + padding on the sides
          width: 'calc(100% - 3rem)',
          // 45% of 1920
          maxWidth: '864px',
          minHeight: '1rem',
          padding: inputSizings.padding.toString(),
          backgroundColor: '#EFECEA',
          borderRadius: '0.5rem',
          fontSize: '14px',
          overflow: 'hidden',
          display: 'flex',
          zIndex: 3,
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
            onKeyDown={sendPrompt}
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
      <div style={{
        position: 'relative',
        width: '40vw',
        margin: '0 auto',
        top: 'calc(2.5rem + 1px)',
        flex: 1,
        marginBottom: `calc(${inputSizings.height.toString()} + 10vh)`
      }}>
        {loadedConversation.messages.map((m) => {
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
          let content = m.content.replace(toPattern, function (match) {
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

              return c.replace(fromPattern, function (match) {
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
                color: '#ABA7A2',
                fontSize: '0.7rem',
                marginTop: '1rem',
                marginLeft: '0.5rem',
                userSelect: 'none',
                marginBottom: isUser ? '' : '-0.25rem'
              }}>{isUser ? 'You' : model.model}</div>
              <div style={{
                backgroundColor: isUser ? '#E2E0DD' : '',
                borderRadius: '0.5rem',
                margin: '0 0.25rem 0.25rem 0.25rem 0.25rem',
                padding: '0.01rem 0',
                width: isUser ? 'fit-content' : '',
                position: 'relative',
                fontSize: '14px',
              }}>
                {isUser ? '' : (
                  <p className="messageOptions" style={{
                    position: 'absolute',
                    transform: 'translateX(calc(-100% - 1rem))',
                    userSelect: 'none',
                    cursor: 'pointer',
                    display: 'flex',
                  }}>
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
                          last.system_prompt = systemPrompt;
                          last.api = model;

                          conversation.messages[conversation.messages.length - 1] = last;

                          setLoadedConversation(conversation);
                        }}>Regenerate</div>
                      </div>
                    </div>
                    <div>•</div>
                  </p>
                )}

                {
                  /* The actual message elements */
                  unescapedElements
                }
              </div>
            </>
          );
        })}
      </div>
    </div>
  );
}

root.render(
  <MainPage />
);
