// Ayarlar sayfası.

import type { settings as en } from "../en/settings";
import type { Messages } from "../types";

export const settings: Messages<typeof en> = {
  title: "Ayarlar",
  description: "Dil, görünüm ve OpenWraith hakkında bilgiler.",
  language: {
    title: "Dil",
    system: "Windows ile aynı",
    english: "English",
    turkish: "Türkçe",
  },
  theme: {
    title: "Tema",
    system: "Windows ile aynı",
    light: "Açık",
    dark: "Koyu",
  },
  safety: {
    title: "Güvenlik yedekleri klasörü",
    body: "OpenWraith her değişiklikten önce buraya bir güvenlik yedeği kaydeder. Komut satırı aracı da aynı klasörü kullanır; OPENWRAITH_SAFETY_DIR ortam değişkeni ikisi için de klasörü değiştirir.",
    show: "Göster",
  },
  about: {
    title: "OpenWraith hakkında",
    version: "Sürüm",
    license: "Lisans",
    licenseValue: "GPL-3.0-or-later",
    repository: "Kaynak kodu",
    copyright: "Telif hakkı (C) 2026 OpenWraith'e katkıda bulunanlar",
    warranty:
      "Bu program HİÇBİR GARANTİ olmaksızın sunulur. Özgür yazılımdır; GNU Genel Kamu Lisansı'nın 3. veya daha sonraki bir sürümünün koşullarıyla yeniden dağıtabilir ve değiştirebilirsiniz.",
    thirdParty:
      "OpenWraith, başka yazarların açık kaynaklı bileşenlerini içerir; her bileşen kendi lisansıyla sunulur. Lisans metinleri kurulum klasöründe, LICENSE.txt dosyasının yanındaki THIRD-PARTY-NOTICES.txt dosyasındadır.",
    showLicenses: "Lisans dosyalarını göster",
    privacy: "Hesap yok, telemetri yok, ağ bağlantısı yok: OpenWraith yalnızca klavyeyle konuşur.",
    cliUndo: "Bir değişikliği komut satırı aracıyla da geri alabilirsiniz:",
    cliUndoCommand: "openwraith restore <güvenlik yedeği dosyası>",
    notIncluded: "Bu sürümde olmayanlar: tuş atamaları, makrolar, tuş başına renkler, yan ışık, SOCD ve yoklama hızı.",
    simulator: "Bu, geliştirme için hazırlanmış bir simülatör sürümü.",
  },
  saveFailed: "Ayar kaydedilemedi.",
};
