// Uygulama çerçevesi: gezinme, üst bilgi, durum göstergesi, uyarı bantları,
// kapatma koruması, bildirimler, simülatör ve önizleme panelleri.

import type { shell as en } from "../en/shell";
import type { Messages } from "../types";

/** Başka bir programın klavyede bıraktıkları, sıfır olanlar olmadan ("a, b ve c"). */
function evidenceText(p: { status: number; acks: number; profiles: number }): string {
  const parts = [
    p.status > 0 ? `${p.status} durum mesajı` : null,
    p.acks > 0 ? `başka bir programın komutlarına verilmiş ${p.acks} yanıt` : null,
    p.profiles > 0 ? `${p.profiles} profil bildirimi` : null,
  ].filter((part): part is string => part !== null);
  if (parts.length === 0) return "mesajlar";
  const last = parts.pop() ?? "";
  return parts.length === 0 ? last : `${parts.join(", ")} ve ${last}`;
}

export const shell: Messages<typeof en> = {
  boot: {
    loading: "OpenWraith başlatılıyor…",
    failedTitle: "OpenWraith arka plan hizmetine ulaşamıyor",
    failedBody:
      "Pencere açıldı ama OpenWraith'in klavyeyle konuşan bölümü yanıt vermedi. Hiçbir şey yazılmadı. Uygulamayı yeniden başlatın; sorun sürerse teknik ayrıntılarla birlikte bildirin.",
  },
  skipToContent: "İçeriğe geç",
  lastReport: {
    title: "Son işlemin sonucu",
    intro: "Bu işlem, sayfası açık değilken bitti. Olanlar aşağıda.",
  },
  nav: {
    label: "Sayfalar",
    device: "Klavye",
    keys: "Tuşlar",
    actuation: "Aktivasyon",
    lighting: "Işık",
    backup: "Yedek ve profiller",
    settings: "Ayarlar",
    unwritten: "Yazılmamış değişiklikler",
  },
  tabs: {
    label: (p) => `${p.page} bölümleri`,
    unwritten: "Bu sekmede yazılmamış değişiklikler var",
    pages: {
      keys: {
        assign: "Atamalar",
        macros: "Makrolar",
        advanced: "Gelişmiş tuşlar",
      },
      actuation: {
        trigger: "Aktivasyon ve Rapid Trigger",
        deadband: "Ölü bölge",
        socd: "SOCD",
        depth: "Tuş derinliği",
      },
      lighting: {
        effect: "Efekt",
        colors: "Tuş renkleri",
      },
      settings: {
        general: "Genel",
        background: "Arka plan",
        games: "Oyunlar",
        profiles: "Profil adları",
        about: "Hakkında",
      },
    },
  },
  sections: {
    share: "Profil paylaş",
  },
  placeholder: {
    title: "Henüz hazır değil",
    body: "OpenWraith'in bu bölümü hâlâ yapım aşamasında. Şimdilik klavyede hiçbir şey okumuyor ve yazmıyor.",
  },
  drafts: {
    where: (p) => `${p.page} > ${p.tab}`,
    entry: (p) => `${p.where}: ${p.count} yazılmamış değişiklik`,
    entryUncounted: (p) => `${p.where}: yazılmamış değişiklikler`,
  },
  header: {
    profileLabel: "Etkin profil",
    profileOption: (p) => `Profil ${p.n}`,
    switcherBlocked: {
      busy: "Meşgul",
      notReady: "Bağlı değil",
      blocked: "Yazma duraklatıldı",
      pausedForCli: "Bir OpenWraith komutu çalışıyor",
    },
    profileOptionNamed: (p) => `Profil ${p.n}, ${p.name}`,
    auto: "Otomatik",
    autoTitle: (p) =>
      `${p.label}, klavyeyi şu profile geçirdi: ${p.profile}. Profili kendiniz seçerseniz oyun kapanana kadar sizin seçiminiz geçerli olur. Ayarlar > Oyunlar'ı açın.`,
    autoPaused: "Otomatik duraklatıldı",
    autoPausedTitle: "Oyunlar için otomatik profiller duraklatıldı. Sürdürmek için Ayarlar > Oyunlar'ı açın.",
    statusButton: "Klavye bağlantısının ayrıntıları",
    live: "Canlı",
    liveTitle: "Tuş derinliği görünümü açık. Durdurmak için Aktivasyon > Tuş derinliği sekmesini açın.",
  },
  status: {
    starting: "Başlatılıyor…",
    searching: "Klavye aranıyor…",
    severalKeyboards: "Birden fazla klavye bağlı",
    opening: "Klavye açılıyor…",
    openFailed: (p) => `Klavye açılamadı · ${p.seconds} sonra yeniden denenecek`,
    observing: "Klavyeyi başka bir programın kullanmadığı denetleniyor…",
    otherProgram: "Klavyeyi başka bir program kullanıyor",
    reading: (p) => `Klavye okunuyor… ${p.done}/${p.total}`,
    readFailed: "Klavye okunamadı",
    ready: (p) => `${p.model} · FW ${p.firmware}`,
    readyNoFirmware: (p) => p.model,
    lost: "Klavyenin bağlantısı kesildi",
    resetSeen: "Klavye kendini sıfırladı",
    unsupported: "Desteklenmeyen klavye",
    released: "Klavye bırakıldı",
    fault: "İç hata: uygulamayı yeniden başlatın",
    simulatorBadge: "Simülatör",
  },
  popover: {
    title: "Klavye",
    connection: "Bağlantı",
    model: "Model",
    product: "Ürün kodu",
    firmware: "Firmware",
    usb: "USB kimliği",
    activeProfile: "Etkin profil",
    readAgain: "Yeniden oku",
    chooseKeyboard: "Klavye seç…",
    release: "Klavyeyi bırak",
    releaseHint: "OpenWraith bağlantısını kapatır; böylece klavyeyi diğer programlar tek başına kullanabilir.",
    connect: "Bağlan",
  },
  busy: {
    working: "Çalışıyor…",
  },
  banners: {
    simulator:
      "Simülatör: gerçek klavye kullanılmıyor. Burada yaptığınız her şey simüle edilmiş bir W75'e gider.",
    faultTitle: "OpenWraith klavyeyi kullanmayı bıraktı",
    faultBody:
      "Bir iç hata oluştu. Uygulamayı yeniden başlatana kadar yazma kapalı. Klavyeye başka hiçbir şey gönderilmeyecek.",
    interruptedTitle: "Önceki bir değişiklik yarıda kaldı",
    interruptedBody: (p) =>
      `${p.when} tarihinde başlayan işlem (${p.what}; profiller: ${p.profiles}) tamamlanmadı: OpenWraith yazarken kapandı ya da çöktü. Klavyede değişikliğin bir kısmı kalmış olabilir. Öncesinde alınan güvenlik yedeğiyle geri alabilirsiniz.`,
    interruptedNoBackup: "Bu işlem için kayıtlı bir güvenlik yedeği yok.",
    resetTitle: "Klavye kendini sıfırladı",
    resetBody:
      "Klavye fabrika ayarlarına döndüğünü bildirdi (Fn + Boşluk). OpenWraith durdu ve hiçbir şey göndermedi. Bir şey değiştirmeden önce klavyeyi yeniden okuyun; ayarlarınızı bir yedekle geri getirebilirsiniz.",
    resetReadAgain: "Klavyeyi yeniden oku",
    resetRestore: "Yedekten geri yükle…",
    otherProgramTitle: "Klavyeyi başka bir program kullanıyor",
    otherProgramBody:
      "Bu büyük olasılıkla wraith.software sitesi. O tarayıcı sekmesini tamamen kapatın; başka bir sekmeye geçmek yetmez. O zamana kadar yazma duraklatıldı.",
    otherProgramCheck: "Kapattım, yeniden denetle",
    otherProgramDetails: "OpenWraith'in gördükleri",
    evidence: (p) => `OpenWraith'in istemediği ${evidenceText(p)} geldi.`,
    safetyDirTitle: "Güvenlik yedeği klasörü kullanılamıyor",
    safetyDirBody:
      "OpenWraith her değişiklikten önce bir güvenlik yedeği alır; şu anda alamadığı için yazma kapalı. Okuma çalışmaya devam ediyor.",
    staleTitle: "Gösterilen değerler güncel olmayabilir",
    staleBody: "Klavye okunduktan sonra klavyede bir şey değişti. Bir şey değiştirmeden önce klavyeyi yeniden okuyun.",
    recoveryTitle: "Klavyenizin ayarları sıfırlanmış olabilir",
    recoveryProfiles: (p) =>
      p.count === 1
        ? `${p.list} numaralı profil, son otomatik yedeğinin aksine şimdi fabrika ayarlarında görünüyor.`
        : `${p.list} numaralı profiller, son otomatik yedeklerinin aksine şimdi fabrika ayarlarında görünüyor.`,
    recoveryReported: "Klavye fabrika ayarlarına döndüğünü bildirdi (Fn + Boşluk basılı tutuldu).",
    recoveryNothingChanged: (p) =>
      p.count === 1
        ? "OpenWraith hiçbir şeyi değiştirmedi ve bu profilin otomatik yedeklerini duraklattı. Onu otomatik yedeklerinizle karşılaştırın ve gerekenleri geri yükleyin."
        : "OpenWraith hiçbir şeyi değiştirmedi ve bu profillerin otomatik yedeklerini duraklattı. Onları otomatik yedeklerinizle karşılaştırın ve gerekenleri geri yükleyin.",
    recoveryNever: "OpenWraith sormadan hiçbir şeyi geri yüklemez.",
    recoveryReview: "İncele ve geri yükle…",
    firmwareTitle: "Klavyenin firmware sürümü değişti",
    firmwareBody:
      "Firmware güncellemesi genellikle klavyenin ayarlarını sıfırlar. Klavyeyi güncellemeden önce alınan otomatik yedeklerle karşılaştırın.",
    pausedForCliTitle: "Klavyeyi bir OpenWraith komutu kullanıyor",
    pausedForCliBody:
      "Komut satırı aracı çalışıyor. OpenWraith komut bitene kadar hiçbir şey göndermez, sonra klavyeyi yeniden okur. Yazma ve oyun profilleri o zamana kadar bekler.",
    pausedForCliPurpose: "Komut",
    autoPausedTitle: "Oyun profilleri duraklatıldı",
    autoPausedRateLimit:
      "Oyun kuralları bir saat içinde profili 30'dan fazla kez değiştirdi; bu yüzden OpenWraith onları duraklattı. Siz sürdürene ya da OpenWraith'i yeniden başlatana kadar duraklatılmış kalırlar.",
    autoPausedFailed:
      "Bir oyun kuralının profil geçişi çalışmadı; bu yüzden OpenWraith oyun profillerini duraklattı. Siz sürdürene ya da OpenWraith'i yeniden başlatana kadar duraklatılmış kalırlar.",
    autoResume: "Sürdür",
    autoOpenSettings: "Ayarlar > Oyunlar'ı aç",
    untestedTitle: "Firmware henüz test edilmedi",
    strayTravelTitle: "Tuş derinliği görünümü klavyede hâlâ açık olabilir",
    strayTravelBody:
      "Önceki bir tuş derinliği görünümü durdurulmamış olabilir (OpenWraith kapandı ya da klavyenin bağlantısı kesildi). Tuşlar normal şekilde çalışmaya devam eder; görünümü Tuş derinliği sekmesinden durdurun.",
    strayTravelOpen: "Tuş derinliğine git",
    untestedBody: (p) =>
      `Bu klavyede ${p.firmware} sürümlü firmware var; OpenWraith ${p.tested} sürümüyle test edildi. Okuma güvenlidir. Her değişiklikten sonra sonucu denetleyin.`,
  },
  closeGuard: {
    title: "OpenWraith klavyeye yazıyor",
    body: "Şimdi kapatırsanız değişiklikler kaydedilmemiş kalabilir. Birkaç saniye içinde bitecek.",
    keepOpen: "Açık kalsın",
    stopAndClose: "Güvenle durdur ve kapat",
    stopping:
      "Güvenle durduruluyor: OpenWraith geçerli adımı bitirecek, klavyenin kaydetmesini bekleyecek ve sonra kapanacak.",
  },
  switcher: {
    title: (p) => `${p.n} numaralı profile geçilsin mi?`,
    body: "Klavye o profilin ışığına geçer; hiçbir ayar değiştirilmez.",
    saveWait: (p) => `OpenWraith önce klavyenin son değişikliğinizi kaydetmesi için ${p.seconds} bekler.`,
    drafts: "Şu yazılmamış değişiklikler atılacak:",
    confirm: (p) => `${p.n} numaralı profile geç`,
    already: (p) => `Klavye zaten profil ${p.n} üzerinde.`,
    autoHolds: (p) =>
      `${p.n} numaralı profili bir oyun kuralı (${p.label}) seçti. Şimdi geçerseniz OpenWraith oyun kapanana kadar sizin seçiminizi korur.`,
  },
  notices: {
    profileChanged: (p) => `Klavyede ${p.to} numaralı profile geçildi`,
    profileChangedBody: "Ayarlar yeniden okundu.",
    connected: "Klavye bağlandı",
    disconnected: "Klavyenin bağlantısı kesildi",
    socdToggled: (p) => (p.on ? "SOCD klavyeden açıldı (Fn + Sağ Shift)." : "SOCD klavyeden kapatıldı (Fn + Sağ Shift)."),
    switched: (p) => `Profil ${p.n} etkin`,
    switchWarnings: "Klavye olağan dışı yanıt verdi; teknik ayrıntılara bakın.",
    autoSwitched: (p) => `${p.label}: profil ${p.n} etkin`,
    autoSwitchedBack: (p) => `Oyun kapandı: profil ${p.n} yeniden etkin`,
    autoWaiting: (p) => `${p.label}: ${p.n} numaralı profile geçiş bekliyor`,
    autoWaitingBack: (p) => `${p.n} numaralı profile geri dönüş bekliyor`,
    autoWaitingBody: (p) => `Bekliyor, çünkü ${p.reason}. 30 saniye sonra atlanır.`,
    autoSkipped: (p) => `${p.label}: ${p.n} numaralı profile geçilmedi`,
    autoSkippedBack: (p) => `${p.n} numaralı profile geri dönüş atlandı`,
    autoSkippedBody: (p) =>
      `OpenWraith 30 saniye bekledi, çünkü ${p.reason}. O profil gerekiyorsa elle geçin.`,
    autoPaused: "Oyun profilleri duraklatıldı",
    autoPausedRateLimit: "Bir saat içinde 30'dan fazla otomatik geçiş oldu.",
    autoPausedFailed: "Bir profil geçişi çalışmadı.",
    resetSuspected: (p) => `Profil ${p.n} sıfırlanmış görünüyor`,
    resetSuspectedBody: "OpenWraith hiçbir şeyi değiştirmedi. Yedek ve profiller sayfasında inceleyin.",
    firmwareChanged: (p) => `Klavyenin firmware sürümü ${p.known} sürümünden ${p.current} sürümüne değişti`,
    firmwareChangedBody:
      "Firmware güncellemeleri genellikle ayarları sıfırlar. Klavyeyi otomatik yedeklerinizle karşılaştırın.",
    settingsInvalid: "OpenWraith'in ayar dosyası okunamadı",
    settingsInvalidKept: (p) =>
      `OpenWraith varsayılan ayarlarla başladı ve eski dosyayı ${p.file} adıyla sakladı.`,
    settingsInvalidLost: "OpenWraith varsayılan ayarlarla başladı.",
    autoBackupFailed: (p) => `Profil ${p.n} için otomatik yedek alınamadı`,
    autoBackupFailedBody: "Klavyedeki ayarlarınız değişmedi. OpenWraith bir sonraki okumada yeniden dener.",
  },
  automation: {
    waitReasons: {
      not_connected: "klavye bağlı değil",
      connecting: "OpenWraith klavyeye hâlâ bağlanıyor",
      busy: "başka bir işlem sürüyor",
      plan_pending: "incelemede açık bir değişiklik var",
      drafts: "yazılmamış değişiklikler var",
      depth_view: "tuş derinliği görünümü açık",
      travel_test: "klavyede bir tuş derinliği testi hâlâ açık olabilir",
      other_program: "klavyeyi başka bir program kullanıyor",
      paused_for_cli: "klavyeyi bir OpenWraith komutu kullanıyor",
      reset_seen: "klavye kendini sıfırladı",
      released: "klavye bırakıldı",
      fault: "OpenWraith bir iç hatadan sonra durdu",
      recent_foreign_activity: "klavyeyi 2 dakikadan kısa süre önce başka bir program kullandı",
      keymap_unsaved: "bir tuş ataması hâlâ kaydediliyor",
      profile_changing: "profil değişiyor",
    },
  },
  devSim: {
    title: "Simülatör denetimleri",
    hint: "Geliştirici araçları: olaylar simüle edilen klavyede, elle yapılmış gibi gerçekleşir.",
    unplug: "Çıkar",
    replug: "Yeniden tak",
    heartbeat: "Üretici sitesi trafiği",
    switchReport: "Fn + 2 (bildirimli)",
    switchSilent: "Fn + 3 (sessiz)",
    reset: "Fabrika ayarlarına sıfırla",
    slowSave: "Yavaş kaydetme (5 sn)",
    dropAck: "Sonraki yanıtı düşür",
    unplugAfter: "3 paketten sonra çıkar",
    info: "Simüle edilen klavye",
    activeProfile: "Etkin profil",
    unsavedEdits: "Kaydedilmemiş değişiklik",
    discardedEdits: "Atılan değişiklik",
    writesSent: "Gönderilen yazma",
    violations: "Kural ihlali",
    unplugged: "Çıkarıldı",
  },
  preview: {
    title: "Önizleme",
    scenario: "Senaryo",
    language: "Dil",
    theme: "Tema",
    speed: "Hız",
    instant: "Anında",
    apply: "Yükle",
    hide: "Gizle",
    show: "Önizleme",
  },
};
