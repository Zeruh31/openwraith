// Işık sayfası (ışık sayfası ajanına ait).

import type { lighting as en } from "../en/lighting";
import type { Messages } from "../types";

export const lighting: Messages<typeof en> = {
  title: "Işık",
  description: "Ana ışığın efekti, rengi, parlaklığı ve hızı.",
  modes: {
    static: "Sabit",
    breath: "Nefes",
    wave: "Dalga",
    neon: "Neon",
    radar: "Radar",
    reactive: "Tepkili",
    aurora: "Kutup ışığı",
    ripple: "Dalgacık",
    twinkle: "Pırıltı",
    custom: "Özel",
    cross: "Çapraz",
    speed_respond: "Hıza duyarlı",
    auto_ripple: "Otomatik dalgacık",
    striation: "Çizgiler",
    fireworks: "Havai fişek",
    music_rhythm: "Müzik ritmi",
    unknown: "Bilinmeyen efekt",
  },
  readAt: (p) => `Okunma saati: ${p.time}`,
  level: (p) => `${p.max} üzerinden ${p.value}`,
  perProfile: {
    title: "Işık ayarları her profil için ayrı saklanır",
    body: (p) =>
      `Şu anda ${p.n} numaralı profilin ışığını düzenliyorsunuz. Her profil kendi ışığını saklar; profil değiştirmek (üst çubuktan ya da Fn + 1–4 ile) ışığı da değiştirir.`,
  },
  empty: {
    title: "Işığı değiştirmek için klavyeyi bağlayın",
    loading: "Klavyenin ışık ayarları okunuyor…",
    goToKeyboard: "Klavye sayfasına git",
  },
  disabled: {
    title: "Işık değiştirilemiyor",
    reasons: {
      not_read: "Işık ayarları klavyeden okunmadı. Klavyeyi yeniden okuyun.",
      invalid_record: "Klavyenin ışık ayarları çözümlenemedi; OpenWraith bu yüzden onlara dokunmuyor.",
      limits_unknown:
        "Klavye hangi efektleri ve değerleri kabul ettiğini bildirmedi; OpenWraith bu yüzden ışığı değiştirmiyor.",
      unsupported_model: "OpenWraith bu klavye modelinin ışığını değiştiremiyor.",
    },
  },
  powerOff: {
    title: "Klavye ışığının kapalı olduğunu bildiriyor",
    body: "Değişiklikler kaydedilir ama ışığı açana kadar görünmez (klavyede Fn + Backspace).",
  },
  effects: {
    title: "Efekt",
    label: "Efekt",
    onKeyboard: "Klavyede etkin",
    unknown: (p) => `Bilinmeyen efekt (${p.id})`,
    unknownHint: "Işığı değiştirmek için bir efekt seçin.",
  },
  controls: {
    title: "Ayarlar",
  },
  color: {
    label: "Renk",
    customNote:
      "Bu sürümde tuş başına renkler düzenlenemez; klavye kayıtlı tuş renklerini göstermeye devam eder.",
    fullColorNote: "Bu efekt tam renk (gökkuşağı) kullanıyor; seçtiğiniz renk görünmeyebilir.",
    names: {
      red: "Kırmızı",
      orange: "Turuncu",
      yellow: "Sarı",
      green: "Yeşil",
      cyan: "Camgöbeği",
      blue: "Mavi",
      purple: "Mor",
      white: "Beyaz",
    },
  },
  brightness: {
    label: "Parlaklık",
    zeroHint: "0 en düşük parlaklıktır; tuşlar kapalı gibi görünebilir.",
  },
  speed: {
    label: "Hız",
    hint: "0 değerinin en yavaş mı yoksa en hızlı mı olduğu henüz bilinmiyor.",
    notUsed: "Bu efektin hız ayarı yok.",
  },
  preview: {
    title: "Önizleme",
    now: "Klavyede şu an",
    after: "Yazıldıktan sonra",
    describe: (p) => `${p.effect}, renk ${p.color}, parlaklık ${p.brightness}`,
    describeCustom: (p) => `${p.effect}, kayıtlı tuş renkleri, parlaklık ${p.brightness}`,
    note: "Yaklaşık, durağan bir görüntüdür; klavyedeki animasyon gösterilmez.",
  },
  applyBar: {
    notWritten: "Henüz yazılmadı",
    noChanges: "Yazılmamış değişiklik yok.",
    effect: (p) => `Efekt: ${p.from} → ${p.to}`,
    color: "Renk",
    brightness: (p) => `Parlaklık: ${p.from} → ${p.to}`,
    speed: (p) => `Hız: ${p.from} → ${p.to}`,
    notReady: "Klavyeyi bağlayın ve okunmasını bekleyin.",
    busy: "Süren işlemin bitmesini bekleyin.",
    notEditable: "Bu klavyenin ışığı değiştirilemiyor.",
    chooseEffect: "Önce bir efekt seçin.",
  },
  review: {
    title: (p) => `${p.n} numaralı profilin ışığı değiştirilsin mi?`,
    intro: (p) =>
      `Bu işlem yalnızca ${p.n} numaralı profilin ışığını değiştirir; diğer profiller kendi ışıklarını korur.`,
    setting: "Ayar",
    now: "Şu an",
    new: "Yeni",
    changes: "(değişiyor)",
    perKey: "Tuş başına renkler",
    notUsed: "Bu efektte kullanılmaz",
    modeChange: "Efektin yönü, yeni efektin varsayılan yönüne döner.",
    packets: (p) => `${p.n} paket`,
    packetsModeChange:
      "2 paket: önce müzik modundan çıkılır (üreticinin sitesi de böyle yapar), sonra ışık kaydı yazılır.",
  },
  discarded: {
    title: "Yazılmamış ışık değişiklikleri atıldı",
    body: "Klavye başka bir profile geçti ya da yeniden bağlandı; her profilin kendi ışığı vardır.",
  },
};
