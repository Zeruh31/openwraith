// Ayarlar sayfası.

import type { settings as en } from "../en/settings";
import type { Messages } from "../types";

export const settings: Messages<typeof en> = {
  title: "Ayarlar",
  description: "Dil, görünüm, hazır ayarlarınız ve ipuçları, ayrıca OpenWraith hakkında bilgiler.",
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
  presets: {
    title: "Tetikleme hazır ayarları",
    body: "Kendi hazır ayarlarınız bu bilgisayarda, OpenWraith'in ayarlarında saklanır. Bunları Aktivasyon ve Rapid Trigger sekmesindeki Hazır ayarlar… ile kaydedin, uygulayın ya da silin.",
    count: (p) =>
      p.n === 0 ? "Henüz bir hazır ayar kaydetmediniz." : `Kaydettiğiniz hazır ayar sayısı: ${p.n}.`,
    open: "Aktivasyon ve Rapid Trigger'ı aç",
  },
  tips: {
    title: "İpuçları",
    body: "OpenWraith bazı ipuçlarını (örneğin Tuşlar sayfasında) Anladım'ı seçene kadar bir kez gösterir.",
    count: (p) => (p.n === 0 ? "Kapatılmış ipucu yok." : `Kapatılan ipucu sayısı: ${p.n}.`),
    reset: "İpuçlarını yeniden göster",
    done: "İpuçları yeniden gösterilecek.",
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
    notIncluded:
      "Bu sürümde olmayanlar: yan ışık, yoklama hızı, switch türleri ve kalibrasyon, makrolarda fare ve medya adımları, bildirim alanı simgesi, Windows ile başlatma, otomatik yedekler, sıfırlama uyarıları ve oyuna göre profiller.",
    simulator: "Bu, geliştirme için hazırlanmış bir simülatör sürümü.",
  },
  saveFailed: "Ayar kaydedilemedi.",
};
