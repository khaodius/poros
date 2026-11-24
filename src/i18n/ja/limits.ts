import type { TranslatedArea } from "../catalog";

export const limits = {
  // components/LimitScheduleSettings
  speedLimitSchedule: "速度制限のスケジュール",
  followTheSchedule: "スケジュールに従う",
  followTheScheduleHint:
    "週の決まった時刻に、別の制限または一時停止を適用します (時刻はこのコンピューターの時刻)。どのルールも当てはまらない時間は、上の制限が適用されます。",
  whereRulesOverlapTheOne:
    "ルールが重なる場合は、一覧で下にあるルールが優先されます。一時停止はその方向の転送を止め、終わると止まったところから続行します。フォルダーの同期、rsync による更新、スケジュールタスクもスケジュールに従います。poros-cli も、独自の制限を指定しない限りスケジュールに従います。",
  addRule: "ルールを追加",
  theScheduleIsOffThe:
    "スケジュールはオフです。週の表示は、オンにしたときにルールがどう働くかを示しています。",
  closeTheRule: "ルールを閉じる",
  editTheRule: "ルールを編集",
  moveUp: "上へ移動",
  moveUp2: "{label} を上へ移動",
  moveDownSoItWins: "下へ移動 (上のルールより優先されます)",
  moveDown: "{label} を下へ移動",
  deleteRule: "ルールを削除",
  delete: "{label} を削除",
  nameOf: "{label} の名前",
  nameSuchAsOfficeHours: "名前 (「業務時間」など)",
  daysOfTheWeek: "曜日",
  startTime: "開始時刻",
  to: "～",
  endTime: "終了時刻",
  limitTo: "制限",
  unlimited: "無制限",
  noDaysSoItNever: "曜日が選ばれていないため、適用されません",
  uploadLimit: "アップロードの上限",
  downloadLimit: "ダウンロードの上限",

  // components/LimitScheduleStatus
  followSchedule: "スケジュールに従う",
  followTheSpeedLimitSchedule: "速度制限のスケジュールに再び従う",
  ignoreSchedule: "スケジュールを無視",
  ignoreTheSpeedLimitSchedule: "次に切り替わるまで速度制限のスケジュールを無視",
  clickToChangeTheSchedule: "{scheduleTooltip}\nクリックするとスケジュールを変更できます。",
  scheduleIgnoredShort: "スケジュール無視中",
  scheduleIgnoredShortUntil: "{time} までスケジュール無視中",
  untilTime: "({time} まで)",

  // components/LimitScheduleWeek
  speedLimitsThroughTheWeek: "1 週間の速度制限",
  eachHourUploadsAboveDownloads: "各時間: 上がアップロード、下がダウンロード",
  now: "現在",
  limited: "制限あり",
  paused: "一時停止",
  nowSpan: "現在: {span}",

  // lib/limitSchedule
  ruleNumber: "ルール {number}",
  kibPerSecond: "{kib} KiB/s",
  unlimitedInASentence: "無制限",
  pausedInASentence: "一時停止",
  uploadsAndDownloads: "アップロードとダウンロード: {limit}",
  uploadsThenDownloads: "アップロード: {upload}、ダウンロード: {download}",
  allDay: "終日",
  twentyFourHoursFrom: "{start} から 24 時間",
  midnight: "24:00",
  fromToTheNextDay: "{start}～翌日 {end}",
  fromTo: "{start}～{end}",
  noRuleTheOrdinaryLimits: "ルールなし (通常の制限)",
  spanDescription: "{weekday} {start}～{end}: {source}、{limits}",
  weekdayAndTime: "{weekday} {time}",
  waitingForTheSpeedLimitSchedule: "速度制限のスケジュールにより待機中",
  waitingForTheSpeedLimitScheduleUntil: "速度制限のスケジュールにより {time} まで待機中",
  scheduleIgnored:
    "速度制限のスケジュールを無視しています。「{rule}」は適用せず、通常の制限を適用します ({limits})。",
  scheduleApplies: "速度制限のスケジュール: 「{rule}」を適用しています ({limits})。",
  scheduleIgnoredUntil:
    "{time} まで速度制限のスケジュールを無視しています。「{rule}」は適用せず、通常の制限を適用します ({limits})。",
  scheduleAppliesUntil:
    "速度制限のスケジュール: {time} まで「{rule}」を適用しています ({limits})。",
} satisfies TranslatedArea<"limits">;
