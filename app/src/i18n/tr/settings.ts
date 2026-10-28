// Ayarlar sayfası: Genel, Arka plan, Oyunlar, Profil adları, Hakkında.

import type { settings as en } from "../en/settings";
import type { Messages } from "../types";

export const settings: Messages<typeof en> = {
  title: "Ayarlar",
  descriptions: {
    general: "Dil, görünüm, hazır ayarlarınız ve ipuçları, ayrıca güvenlik yedekleri klasörü.",
    background: "OpenWraith'in arka planda çalışması, bildirimler ve otomatik yedekler.",
    games: "Bir oyun öne geldiğinde klavyeyi o oyunun profiline geçirin.",
    profiles: "Dört profil için üst bilgide ve menülerde gösterilen adlar.",
    about: "Sürüm, lisans ve bu sürümün yapmadıkları.",
  },
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
  background: {
    trayTitle: "Bildirim alanı",
    tray: "OpenWraith bildirim alanında çalışmaya devam etsin",
    trayHelp:
      "Böylece pencereyi kapattığınızda OpenWraith saatin yanında çalışmaya devam eder; oyun profilleri ve sıfırlama uyarıları çalışmayı sürdürür. Çıkmak için simgenin menüsünden Çık'ı seçin.",
    startWithWindows: "OpenWraith Windows ile başlasın",
    startWithWindowsHelp: "Bildirim alanında gizli olarak başlar.",
    startNeedsTray: "Önce bildirim alanı simgesini açın: OpenWraith Windows ile yalnızca orada başlar.",
    startUninstall:
      "Simgeyi kapatmak, OpenWraith'i Windows ile başlayan programlardan da çıkarır. OpenWraith'i kurulum programıyla (.exe) kurduysanız kaldırma işlemi bu kaydı da siler; .msi paketiyle kurduysanız kaldırmadan önce bu seçeneği kapatın.",
    windowsStart: {
      offInTaskManagerTitle: "Windows OpenWraith'i başlatmayacak: Görev Yöneticisi'nde kapatılmış",
      offInTaskManagerBody:
        "OpenWraith, Görev Yöneticisi > Başlangıç uygulamaları bölümünde kapatılmış. OpenWraith bu seçimi hiçbir zaman değiştirmez; anahtarı kapatıp açmak da değiştirmez: OpenWraith'i orada (ya da Windows Ayarlar > Uygulamalar > Başlangıç bölümünde) açın.",
      entryMissingTitle: "Windows OpenWraith'i başlatmayacak: başlangıç kaydı yok",
      entryMissingBody:
        "Windows'ta OpenWraith'in bu kopyası için bir başlangıç kaydı yok: kayıt silinmiş ya da başka bir kopyayı başlatıyor. Kaydı yeniden yazmak için anahtarı kapatıp yeniden açın.",
      stillOnTitle: "Windows OpenWraith'i hâlâ başlatıyor",
      stillOnBody:
        "Anahtar kapalı, ancak OpenWraith'in başlangıç kaydı silinemedi. Anahtarı açıp yeniden kapatın ya da OpenWraith'i Görev Yöneticisi > Başlangıç uygulamaları bölümünden kapatın.",
      stillOnNoTrayBody:
        "Bildirim alanı simgesi kapalı, ancak OpenWraith'in başlangıç kaydı silinemedi. OpenWraith'i Görev Yöneticisi > Başlangıç uygulamaları bölümünden kapatın ya da OpenWraith'i kapatıp yeniden başlatın: açılırken kaydı siler.",
      unavailableTitle: "OpenWraith, Windows'un onu başlatıp başlatmayacağını bilemiyor",
      unavailableBody:
        "Başlangıç kaydı okunamadı ya da OpenWraith'in bu kopyası için kayıt yazılamıyor (örneğin klasör yolu çok uzun). OpenWraith'i Görev Yöneticisi > Başlangıç uygulamaları bölümünde denetleyin ya da anahtarı kapatıp yeniden açın: bir ileti neyin yanlış gittiğini söyler.",
      recheck: "Yeniden denetle",
    },
    notificationsTitle: "Bildirimler",
    notifyReset: "Klavye sıfırlanmış göründüğünde ya da firmware sürümü değiştiğinde bana haber ver",
    notifyResetHelp: "Otomatik yedek alınamadığında da.",
    notifyAutoSwitch: "Bir oyun profili değiştirdiğinde bana haber ver",
    notifyAutoSwitchHelp:
      "Bir oyun profili değiştirdiğinde bir Windows bildirimi (en fazla 10 dakikada bir); her geçiş, pencere açıkken OpenWraith içinde de gösterilir.",
    notificationsNote:
      "Rahatsız etmeyin açıkken Windows bildirimleri bekletebilir. OpenWraith'teki uyarı bantları ve bildirim alanı simgesi, ilgilenmeniz gerekenleri her zaman gösterir.",
    autoBackupTitle: "Otomatik yedekler",
    autoBackup: "Her profili otomatik olarak yedekle",
    autoBackupHelp:
      "OpenWraith okuduğu her profilin bir kopyasını saklar ve sonraki okumaları onunla karşılaştırır; böylece klavye sıfırlanmış göründüğünde size haber verebilir. Zaten yaptığı okumaları kullanır: klavyeye fazladan hiçbir şey gönderilmez.",
    autoBackupOff: "Bu seçenek kapalıyken OpenWraith bir sıfırlamayı da fark edemez.",
    keep: "Profil başına saklanan otomatik yedek",
    keepHelp: (p) =>
      `Her profilin en yeni ${p.n} yedeği saklanır. Sabitlenmiş otomatik yedekler ve güvenlik yedekleri hiçbir zaman silinmez.`,
    keepInvalid: "1 ile 100 arasında bir tam sayı girin.",
    keepSave: "Kaydet",
    showBackups: "Otomatik yedekleri göster",
    loadFailed: "Arka plan ayarları okunamadı.",
    shellErrorTitle: "Kaydedildi, ancak Windows değişikliği uygulamadı",
    shellErrorBody:
      "OpenWraith bildirim alanı simgesini ya da Windows ile başlama kaydını güncelleyemedi; ayrıntılar aşağıda. Windows anahtarın dediğini yapmayacaksa, anahtarın altındaki not bunun nasıl düzeltileceğini söyler.",
  },
  games: {
    toggle: "Oyunlar için profili otomatik değiştir",
    toggleHelp: "Buraya eklediğiniz bir oyun öne geldiğinde OpenWraith klavyeyi o oyunun profiline geçirir.",
    needsTray:
      "Oyun profilleri yalnızca OpenWraith çalışırken çalışır. Pencereyi kapattıktan sonra da çalışmaları için bildirim alanı simgesini açın.",
    openBackground: "Arka planı aç",
    defaultLabel: "Bir oyun kapandığında",
    defaultStay: "Oyunun profilinde kal",
    defaultOption: (p) => `Şu profile dön: ${p.label}`,
    defaultHelp:
      "Yalnızca oyunun profiline OpenWraith geçtiyse ve bu profil hâlâ etkinse: kendi seçtiğiniz bir profil hiçbir zaman değiştirilmez.",
    pause: "Duraklat",
    resume: "Sürdür",
    pauseHelp: "Duraklatma, siz sürdürene ya da OpenWraith'i yeniden başlatana kadar sürer.",
    statusLabel: "Şu anda",
    line: {
      off: "Oyun profilleri kapalı.",
      paused: (p) => `Duraklatıldı: ${p.reason}.`,
      noRules: "Açık, ancak henüz oyun eklenmedi.",
      watching: "Oyunlarınız izleniyor.",
      dwelling: (p) => `${p.label} önde; az sonra geçilecek profil: ${p.profile}.`,
      switched: (p) => `${p.label} önde; etkin profil: ${p.profile}.`,
      overridden: (p) =>
        `${p.label} önde. ${p.profile} yerine başka bir profil seçtiniz; OpenWraith oyun kapanana kadar seçiminize dokunmaz.`,
      skipped: (p) =>
        `${p.label} önde, ancak bu profile geçiş atlandı: ${p.profile}. Oyun bir sonraki açılışında yeniden denenir.`,
      waiting: (p) => `${p.label}: geçilecek profil ${p.profile}; bekliyor, çünkü ${p.reason}.`,
      waitingBack: (p) => `Geri dönülecek profil ${p.profile}; bekliyor, çünkü ${p.reason}.`,
    },
    pauseReasons: {
      user: "siz duraklattınız",
      rate_limit: "bir saat içinde 30'dan fazla otomatik geçiş oldu",
      switch_failed: "bir profil geçişi çalışmadı",
    },
    rules: {
      title: "Oyunlarınız",
      caption: "Oyunlar ve profilleri",
      empty: "Henüz oyun yok. Kendi profili olması gereken oyunları ekleyin.",
      game: "Oyun",
      program: "Program dosyası",
      profile: "Profil",
      on: "Otomatik geçiş",
      remove: "Kaldır",
      anyFolder: "Herhangi bir klasör",
      ruleOn: (p) => `${p.label} için geçiş`,
      profileFor: (p) => `${p.label} için profil`,
      removeRule: (p) => `${p.label} oyununu kaldır`,
      removed: (p) => `${p.label} kaldırıldı.`,
      generic: "Bu adı birçok program kullanır; bu yüzden kural hepsi için profili değiştirir.",
      genericShort: "Ortak ad",
      add: "Oyun ekle…",
    },
    add: {
      title: "Oyun ekle",
      program: "Program dosyası",
      none: "Henüz program seçilmedi.",
      running: "Çalışan programlar",
      runningShow: "Çalışan bir program seç",
      runningEmpty: "Seçilecek program yok: oyunu başlatın, sonra yenileyin.",
      refresh: "Yenile",
      unavailable:
        "OpenWraith'in bu sürümü çalışan programları göremiyor. Programa göz atın ya da dosya adını yazın.",
      browse: "Göz at…",
      browseTitle: "Bir oyun seçin",
      browseFilter: "Programlar",
      detect: "Açacağım sonraki programı algıla",
      detecting: "Şimdi oyununuza geçin: OpenWraith öne gelen sonraki programı alır.",
      detectLeft: (p) => `Kalan süre: ${p.time}`,
      detectNone: "30 saniye içinde öne hiçbir program gelmedi. Yeniden deneyin ya da programı başka bir yolla seçin.",
      detectStop: "Beklemeyi bırak",
      typeName: "Ya da dosya adını yazın",
      typeNameHint: "Örneğin oyun.exe. Kural bu adı her klasörde eşleştirir.",
      typeNameInvalid: "Klasör olmadan bir dosya adı girin: \\, / ya da : kullanmayın.",
      useName: "Bu adı kullan",
      folder: (p) => `Klasör: ${p.folder}`,
      folderUnknown:
        "Klasör bilinmiyor (program yönetici olarak çalışıyor olabilir): kural bu adı her klasörde eşleştirir.",
      name: "Ad",
      nameHint: "Oyunun OpenWraith'te görünen adı.",
      profile: "Profil",
      confirm: "Oyunu ekle",
      needProgram: "Önce bir program seçin.",
      needName: "Bir ad girin.",
      failed: "Oyun eklenemedi.",
    },
    recent: {
      title: "Son etkinlikler",
      empty: "Henüz bir şey yok.",
      aGame: "Bir oyun",
      switched: (p) => `${p.label}: profil değiştirildi, ${p.from} → ${p.to}.`,
      switchedTo: (p) => `${p.label}: geçilen profil ${p.to}.`,
      switchedBack: (p) => `Bir oyun kapandı; geri dönülen profil ${p.to}.`,
      deferred: (p) => `${p.label}: ${p.profile} için geçiş bekledi, çünkü ${p.reason}.`,
      deferredBack: (p) => `${p.profile} için geri dönüş bekledi, çünkü ${p.reason}.`,
      skipped: (p) => `${p.label}: ${p.profile} için geçiş atlandı, çünkü ${p.reason}.`,
      skippedBack: (p) => `${p.profile} için geri dönüş atlandı, çünkü ${p.reason}.`,
      overridden: (p) =>
        `${p.label}: siz başka bir profil seçtiniz (${p.profile}); OpenWraith oyun kapanana kadar ona dokunmaz.`,
      overriddenNoGame: (p) => `Siz başka bir profil seçtiniz (${p.profile}).`,
      paused: (p) => `Duraklatıldı: ${p.reason}.`,
      resumed: "Sürdürüldü.",
    },
    howItWorks: {
      title: "Nasıl çalışır",
      writes:
        "OpenWraith yalnızca profil geçişini yazar; bu, Fn + 1-4'e basmakla aynıdır. Ayarlarınız olduğu gibi kalır.",
      waits:
        "Bir oyun bir süre önde kaldıktan sonra, klavye son değişikliğinizi kaydettiğinde geçiş yapar. Yazılmamış değişiklikleriniz varken, bir inceleme açıkken ya da tuş derinliği görünümü çalışırken hiçbir zaman geçiş yapmaz.",
      stops:
        "Wraith web sitesi gibi başka bir program klavyeyi kullandığında durur. OpenWraith'te “Kapattım, yeniden denetle”yi seçtiğinizde, o program son duyulduktan en az 2 dakika sonra yeniden başlar. Bildirim alanı simgesi açıksa, klavyede 3 dakika hiçbir etkinlik olmadığında kendiliğinden de yeniden başlar. Oynarken bütün wraith.software sekmelerini kapatın.",
      manual:
        "Kendi seçtiğiniz bir profil (Fn + 1-4, üst bilgi ya da bildirim alanı simgesi) oyun kapanana kadar her zaman geçerli olur.",
      exit: "Bir oyun kapandığında OpenWraith, “Bir oyun kapandığında” altında seçilen profile döner; ancak yalnızca oyunun profiline OpenWraith geçtiyse ve bu profil hâlâ etkinse.",
      limits:
        "Bir saat içinde 30 otomatik geçişten sonra ya da bir geçiş çalışmadığında, oyun profilleri siz sürdürene kadar duraklatılır.",
      privacy: "OpenWraith yalnızca hangi programın önde olduğuna bakar. Bilgisayarınızdan hiçbir şey çıkmaz.",
    },
    consent: {
      title: "Oyunlar için profil otomatik değiştirilsin mi?",
      intro: "Bunu açmadan önce:",
      tick: "Anladım. Oyunlar profili değiştirirken bütün wraith.software sekmelerini kapalı tutacağım; otomatik geçişler bunu her seferinde sormaz.",
      confirm: "Aç",
      needsTick: "Oyun profillerini açmak için kutuyu işaretleyin.",
    },
    saveFailed: "Oyun ayarları kaydedilemedi.",
    loadFailed: "Oyun ayarları okunamadı.",
  },
  profiles: {
    intro:
      "Dört profile “İş” ya da “FPS” gibi adlar verin. Adlar bu bilgisayarda kalır: klavye adları saklamaz. Üst bilgide, bildirim alanı menüsünde ve Yedek ve profiller sayfasında gösterilir.",
    product: (p) => `Klavye modeli ${p.product}`,
    connected: "Şu anda bağlı",
    name: (p) => `Profil ${p.n} adı`,
    placeholder: "Ad yok",
    hint: "En çok 32 karakter. Ad vermemek için alanı boş bırakın.",
    save: "Adları kaydet",
    saved: "Adlar kaydedildi.",
    empty: "Klavyenizi bir kez bağlayın; sonra profillerine buradan ad verebilirsiniz.",
    saveFailed: "Adlar kaydedilemedi.",
    loadFailed: "Profil adları okunamadı.",
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
      "Bu sürümde olmayanlar: yan ışık, yoklama hızı, switch türleri ve kalibrasyon, Win-Lock, oyun kumandası (analog) modu, makrolarda fare ve medya adımları.",
    simulator: "Bu, geliştirme için hazırlanmış bir simülatör sürümü.",
  },
  saveFailed: "Ayar kaydedilemedi.",
};
