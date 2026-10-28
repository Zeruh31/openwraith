// Klavye (cihaz durumu) sayfası.

import type { device as en } from "../en/device";
import type { Messages } from "../types";

export const device: Messages<typeof en> = {
  title: "Klavye",
  description: "Wraith W75 klavyenizin bağlantısı, modeli ve firmware sürümü.",
  connection: {
    title: "Bağlantı",
    starting: "OpenWraith başlatılıyor.",
    searching: "Wraith W75 klavyenizi USB kablosuyla bağlayın. OpenWraith onu kendiliğinden bulur.",
    severalKeyboards: "Birden fazla klavye bağlı. OpenWraith'in hangisini kullanacağını seçin.",
    choose: (p) => `${p.n} numaralı klavyeyi kullan`,
    deviceTitle: (p) => `Klavye ${p.n}`,
    deviceId: (p) => `Bağlantı kimliği …${p.id}`,
    deviceLine: (p) => `${p.product} · USB ${p.usb}`,
    serial: (p) => `Seri numarası: ${p.serial}`,
    opening: "Klavye açılıyor…",
    openFailed: "OpenWraith klavyeyi açamadı.",
    retryingIn: (p) => `${p.seconds} sonra yeniden denenecek.`,
    observing:
      "OpenWraith bir şey göndermeden önce, klavyeyle başka bir programın (örneğin wraith.software sitesinin) konuşmadığından emin olmak için kısa bir süre dinler.",
    otherProgram: "Duraklatıldı: klavyeyi başka bir program kullanıyor (yukarıdaki uyarıya bakın). OpenWraith hiçbir şey göndermez.",
    reading: "Klavyenin ayarları okunuyor. Bu birkaç saniye sürer.",
    readFailed: "OpenWraith klavyenin ayarlarını okuyamadı. Hiçbir şey yazılmadı.",
    ready: "Bağlı. Siz bir şey değiştirmediğiniz sürece OpenWraith klavyeyi yalnızca dinler, hiçbir şey göndermez.",
    pausedForCli:
      "Bağlı ve duraklatıldı: klavyeyi bir OpenWraith komutu (komut satırı aracı) kullanıyor. OpenWraith komut bitene kadar hiçbir şey göndermez, sonra klavyeyi yeniden okur.",
    lost: "Klavyenin bağlantısı kesildi. Yeniden takın; OpenWraith kendiliğinden bağlanır.",
    resetSeen:
      "Klavye fabrika ayarlarına döndü (Fn + Boşluk). OpenWraith durdu ve hiçbir şey göndermedi. Devam etmek için klavyeyi yeniden okuyun.",
    unsupported: (p) =>
      `Bu klavye (${p.product}) desteklenmiyor. OpenWraith yalnızca Wraith W75 ile çalışır.`,
    released: "OpenWraith klavyeyi bıraktı. Artık klavyeyi diğer programlar tek başına kullanabilir.",
    fault: "OpenWraith bir iç hatayla karşılaştı ve klavyeyi kullanmayı bıraktı. Uygulamayı yeniden başlatın.",
    connect: "Bağlan",
    reconnect: "Klavyeyi yeniden oku",
    release: "Klavyeyi bırak",
  },
  keyboard: {
    title: "Klavye",
    model: "Model",
    product: "Ürün kodu",
    firmware: "Firmware",
    tested: "Test edildi",
    notTested: "Henüz test edilmedi",
    buildDate: "Firmware derleme tarihi",
    usb: "USB kimliği",
    activeProfile: "Etkin profil",
    pollRate: "Yoklama hızı",
    distanceUnit: "Mesafe birimi",
    readAt: (p) => `Değerlerin okunma saati: ${p.time}`,
    notRead: "Henüz okunmadı",
    undecoded: (p) =>
      `${p.n} yanıt çözümlenemedi; ham veri olarak saklanıyor.`,
  },
  beforeYouStart: {
    title: "Bir şey değiştirmeden önce",
    step1: "Bütün wraith.software sekmelerini kapatın. Başka bir sekmeye geçmek yetmez.",
    step2: "Her zaman geri dönebilmek için dört profilin hepsini bir kez yedekleyin.",
    step2Action: "Yedek ve profiller sayfasına git",
    step3: "Her değişiklikten önce kendiliğinden yedek alınır ve değişiklik geri alınabilir.",
    dismiss: "Anladım",
  },
  safetyFolder: {
    title: "Güvenlik yedekleri",
    body: "OpenWraith her değişiklikten önce klavyenin ayarlarını buraya kaydeder. Komut satırı aracı da aynı klasörü kullanır.",
    unavailable: "Klasör kullanılamıyor; bu yüzden yazma kapalı.",
  },
};
