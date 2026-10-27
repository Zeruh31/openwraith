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
    intro: "Bu işlem, penceresi kapalıyken ya da yeniden yüklenirken bitti. Olanlar aşağıda.",
  },
  nav: {
    label: "Sayfalar",
    device: "Klavye",
    actuation: "Aktivasyon ve Rapid Trigger",
    lighting: "Işık",
    backup: "Yedekle ve geri yükle",
    settings: "Ayarlar",
    unwritten: "Yazılmamış değişiklikler",
  },
  header: {
    profileLabel: "Etkin profil",
    profileOption: (p) => `Profil ${p.n}`,
    switcherBlocked: {
      busy: "Meşgul",
      notReady: "Bağlı değil",
      blocked: "Yazma duraklatıldı",
    },
    statusButton: "Klavye bağlantısının ayrıntıları",
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
    releaseHint: "OpenWraith bağlantıyı kapatır; klavyeyi yalnızca diğer programlar kullanır.",
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
    untestedTitle: "Firmware henüz test edilmedi",
    untestedBody: (p) =>
      `Bu klavyede ${p.firmware} sürümlü firmware var; OpenWraith ${p.tested} sürümüyle denendi. Okumak güvenli. Her değişiklikten sonra sonucu denetleyin.`,
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
    body: "Klavyenin ışığı o profilinkine döner; hiçbir ayar değişmez.",
    saveWait: (p) => `OpenWraith önce klavyenin son değişikliğinizi kaydetmesi için ${p.seconds} bekler.`,
    drafts: (p) => `Şu sayfalardaki yazılmamış değişiklikler atılacak: ${p.pages}.`,
    confirm: () => "Profile geç",
    already: (p) => `Klavye zaten profil ${p.n} üzerinde.`,
  },
  notices: {
    profileChanged: (p) => `Klavyede ${p.to} numaralı profile geçildi`,
    profileChangedBody: "Ayarlar yeniden okundu.",
    connected: "Klavye bağlandı",
    disconnected: "Klavyenin bağlantısı kesildi",
    switched: (p) => `Profil ${p.n} etkin`,
    switchWarnings: "Klavye olağan dışı yanıt verdi; teknik ayrıntılara bakın.",
  },
  devSim: {
    title: "Simülatör denetimleri",
    hint: "Geliştirici araçları: olaylar simüle edilen klavyede, elle yapılmış gibi gerçekleşir.",
    unplug: "Çıkar",
    replug: "Yeniden tak",
    heartbeat: "Üretici sitesi trafiği",
    switchReport: "Fn + 2 (bildirimli)",
    switchSilent: "Fn + 3 (sessiz)",
    reset: "Fabrika ayarları",
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
