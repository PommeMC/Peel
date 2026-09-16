export default function Brand({ large = false }: { large?: boolean }) {
  return (
    <div className={large ? "brand brand-large" : "brand"}>
      <img src="./favicon.svg" alt="" />
      <span>Peel</span>
    </div>
  );
}
