import type { Field } from "../lib/trace";

export interface FieldTreeProps {
  fields: Field[];
  /** The field under the pointer, or null once it leaves the tree. */
  onHover(field: Field | null): void;
}

export default function FieldTree({ fields, onHover }: FieldTreeProps) {
  return (
    <ul className="field-tree" onMouseLeave={() => onHover(null)}>
      <FieldItems fields={fields} onHover={onHover} />
    </ul>
  );
}

function FieldItems({ fields, onHover }: FieldTreeProps) {
  return fields.map((field, i) => {
    const row = (
      <span className="field-row" onMouseEnter={() => onHover(field)}>
        <span className="field-name">{field.name}</span>
        {field.value !== undefined && (
          <span className="field-value">{field.value}</span>
        )}
      </span>
    );
    return (
      <li key={i}>
        {field.children ? (
          <details open>
            <summary>{row}</summary>
            <ul>
              <FieldItems fields={field.children} onHover={onHover} />
            </ul>
          </details>
        ) : (
          row
        )}
      </li>
    );
  });
}
