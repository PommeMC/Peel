import type { Field } from "../lib/trace";

interface Props {
  fields: Field[];
  /** Paths, like `hit_result.cursor`, of fields to mark as changed. */
  changed?: ReadonlySet<string>;
  /** The field under the pointer, or null once it leaves the tree. */
  onHover(field: Field | null): void;
}

export default function FieldTree(props: Props) {
  return (
    <ul className="field-tree" onMouseLeave={() => props.onHover(null)}>
      <FieldItems {...props} prefix="" />
    </ul>
  );
}

function FieldItems({
  fields,
  changed,
  onHover,
  prefix,
}: Props & { prefix: string }) {
  return fields.map((field, i) => {
    const path = prefix + field.name;
    const differs = [...(changed ?? [])].some(
      (p) => p === path || p.startsWith(`${path}.`),
    );
    const row = (
      <span
        className={`field-row${differs ? " changed" : ""}`}
        onMouseEnter={() => onHover(field)}
      >
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
              <FieldItems
                fields={field.children}
                changed={changed}
                onHover={onHover}
                prefix={`${path}.`}
              />
            </ul>
          </details>
        ) : (
          row
        )}
      </li>
    );
  });
}
