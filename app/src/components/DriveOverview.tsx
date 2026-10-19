import { useTranslation } from "react-i18next";
import { formatBytes, formatManufactureDate, hoursToYears, type ScoreTone } from "../lib/format";
import type { DriveInfo, TestReport } from "../lib/types";

/** "GOOD 87 / 100" tile in the style of CrystalDiskInfo. */
export function HealthTile({ tone, score }: { tone: ScoreTone | "none"; score: number | null }) {
  const { t } = useTranslation();
  const word = tone === "good" ? t("health.good") : tone === "caution" ? t("health.caution") : tone === "bad" ? t("health.bad") : t("health.untested");
  return (
    <div className={`health-tile ${tone}`} role="img" aria-label={score != null ? `${word}. ${t("results.scoreLabel", { score })}` : word}>
      <div className="ht-word" aria-hidden="true">{word}</div>
      <div className="ht-score" aria-hidden="true">
        {score != null ? <bdi dir="ltr"><span data-testid="score">{score}</span> / 100</bdi> : <bdi dir="ltr">— / 100</bdi>}
      </div>
    </div>
  );
}

/** Two-column properties table (Model, Serial, ...). Uses the report's drive/age data when available. */
export function Properties({ drive, report }: { drive: DriveInfo; report?: TestReport }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const dash = "—";
  const age = report?.age;
  const hours = age?.power_on_hours ?? drive.power_on_hours;
  const years = age?.power_on_years ?? hoursToYears(hours);
  const cycles = age?.power_cycles ?? null;
  const made = formatManufactureDate(age?.manufacture_date ?? drive.manufacture_date, lang);
  // third item: keep left-to-right (model numbers, sizes) even inside a right-to-left window
  type Row = [string, string, boolean?];
  const left: Row[] = [
    [t("props.model"), drive.model ?? t("picker.unknownModel"), !!drive.model],
    [t("props.serial"), drive.serial?.trim() || dash, true],
    [t("props.firmware"), drive.firmware?.trim() || dash, true],
    [t("props.capacity"), formatBytes(drive.capacity_bytes, lang), true],
    [t("props.interface"), drive.bus ? drive.bus.toUpperCase() : dash, true],
    [t("props.type"), t(`picker.kind.${drive.kind}`)],
  ];
  const right: Row[] = [
    [t("props.rpm"), drive.kind === "hdd" && drive.rotation_rpm ? `${drive.rotation_rpm} RPM` : dash, true],
    [t("props.hours"), hours != null ? t("results.age.hours", { count: hours, hours: hours.toLocaleString(lang) }) : dash],
    [t("props.count"), cycles != null ? t("results.age.cycles", { count: cycles }) : dash],
    [t("props.age"), years != null ? (years < 1 ? t("results.age.yearsLessThanOne") : t("results.age.years", { count: years })) : dash],
    [t("props.made"), made ?? dash],
    [t("picker.systemDisk"), drive.is_system_disk ? t("results.can.yes") : t("results.can.no")],
  ];
  const col = (rows: Row[]) => (
    <table className="props">
      <tbody>
        {rows.map(([k, v, ltr]) => (
          <tr key={k}>
            <th scope="row">{k}</th>
            <td><div className="val" title={v}>{ltr ? <bdi dir="ltr">{v}</bdi> : v}</div></td>
          </tr>
        ))}
      </tbody>
    </table>
  );
  return <div className="props-cols">{col(left)}{col(right)}</div>;
}
