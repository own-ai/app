import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface MessageContentProps {
  content: string;
  role: "user" | "agent" | "system";
}

export const MessageContent = ({ content, role }: MessageContentProps) => {
  // User and system messages are plain text
  if (role === "user" || role === "system") {
    return <div className="whitespace-pre-wrap">{content}</div>;
  }

  // Agent messages support Markdown
  return (
    <ReactMarkdown
      components={{
        // Code blocks
        code(props) {
          const { node, className, children, ...rest } = props;
          const match = /language-(\w+)/.exec(className || "");
          return match ? (
            <SyntaxHighlighter
              style={oneDark}
              language={match[1]}
              PreTag="div"
              className="rounded-lg my-4"
            >
              {String(children).replace(/\n$/, "")}
            </SyntaxHighlighter>
          ) : (
            <code
              className="px-1.5 py-0.5 rounded bg-code-bg font-mono text-sm"
              {...rest}
            >
              {children}
            </code>
          );
        },
        // Links
        a(props) {
          const { node, children, ...rest } = props;
          return (
            <a
              className="text-accent hover:text-accent-hover underline"
              target="_blank"
              rel="noopener noreferrer"
              {...rest}
            >
              {children}
            </a>
          );
        },
        // Paragraphs
        p(props) {
          const { node, children, ...rest } = props;
          return (
            <p className="mb-4 last:mb-0" {...rest}>
              {children}
            </p>
          );
        },
        // Headings
        h1(props) {
          const { node, children, ...rest } = props;
          return (
            <h1 className="text-2xl font-semibold mb-4 mt-6" {...rest}>
              {children}
            </h1>
          );
        },
        h2(props) {
          const { node, children, ...rest } = props;
          return (
            <h2 className="text-xl font-semibold mb-3 mt-5" {...rest}>
              {children}
            </h2>
          );
        },
        h3(props) {
          const { node, children, ...rest } = props;
          return (
            <h3 className="text-lg font-semibold mb-2 mt-4" {...rest}>
              {children}
            </h3>
          );
        },
        // Lists
        ul(props) {
          const { node, children, ...rest } = props;
          return (
            <ul className="list-disc list-inside mb-4 space-y-1" {...rest}>
              {children}
            </ul>
          );
        },
        ol(props) {
          const { node, children, ...rest } = props;
          return (
            <ol className="list-decimal list-inside mb-4 space-y-1" {...rest}>
              {children}
            </ol>
          );
        },
        // Blockquotes
        blockquote(props) {
          const { node, children, ...rest } = props;
          return (
            <blockquote
              className="border-l-4 border-border-strong pl-4 italic my-4"
              {...rest}
            >
              {children}
            </blockquote>
          );
        },
      }}
    >
      {content}
    </ReactMarkdown>
  );
};
