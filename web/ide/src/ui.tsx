import React from "react";

export function MetricCard({
  label,
  value,
  tone = "neutral",
  detail,
}: {
  label: string;
  value: string;
  tone?: "neutral" | "positive" | "negative";
  detail?: string;
}) {
  return (
    <article className={`metric-card metric-card--${tone}`}>
      <span className="metric-card__label">{label}</span>
      <strong className="metric-card__value">{value}</strong>
      {detail ? <span className="metric-card__detail">{detail}</span> : null}
    </article>
  );
}

export function LineChart({
  series,
  height = 180,
}: {
  series: Array<{
    values: number[];
    stroke: string;
    fill?: string;
  }>;
  height?: number;
}) {
  const width = 560;
  const values = series.flatMap((entry) => entry.values);
  if (values.length < 2) {
    return <div className="empty-state">Not enough points.</div>;
  }

  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;

  return (
    <svg className="equity-chart" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
      <rect
        className="equity-chart__bg"
        height={height}
        rx="18"
        width={width}
        x="0"
        y="0"
      />
      {series.map((entry, index) => {
        const path = entry.values
          .map((point, pointIndex) => {
            const x = (pointIndex / Math.max(entry.values.length - 1, 1)) * width;
            const y = height - ((point - min) / span) * (height - 12) - 6;
            return `${pointIndex === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
          })
          .join(" ");
        return (
          <g key={index}>
            {entry.fill ? (
              <path
                d={`${path} L ${width} ${height} L 0 ${height} Z`}
                fill={entry.fill}
                opacity={0.7}
              />
            ) : null}
            <path
              d={path}
              fill="none"
              stroke={entry.stroke}
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={index === 0 ? 3 : 2}
            />
          </g>
        );
      })}
    </svg>
  );
}
