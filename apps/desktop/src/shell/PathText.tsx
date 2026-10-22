import { Fragment } from "react";

/**
 * Text that may hold a file path. A line may break after each `\` or `/`, so a long path wraps
 * between folder names instead of in the middle of one.
 */
export function PathText({ text }: { text: string }) {
  const parts = text.split(/(?<=[\\/])/);
  return (
    <>
      {parts.map((part, index) => (
        <Fragment key={index}>
          {index > 0 ? <wbr /> : null}
          {part}
        </Fragment>
      ))}
    </>
  );
}
