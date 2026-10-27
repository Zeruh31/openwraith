// Yedekle ve geri yükle sayfası (yedekleme sayfası ajanına ait): yedekleme,
// geri yükleme sihirbazı, planın incelenmesi, geri yükleme raporu ve
// güvenlik yedeklerinin listesi.

import type { backupRestore as en } from "../en/backupRestore";
import type { Messages } from "../types";

export const backupRestore: Messages<typeof en> = {
  title: "Yedekle ve geri yükle",
  description: "Klavyenin ayarlarını bir dosyaya kaydedin ve daha sonra geri getirin.",
  reasons: {
    notConnected: "Klavyeyi bağlayın ve okunmasını bekleyin.",
    busy: "Süren işlemin bitmesini bekleyin.",
    otherProgram:
      "Klavyeyi başka bir program kullanıyor. Bütün wraith.software sekmelerini kapatın, sonra yeniden denetleyin.",
    resetSeen: "Klavye kendini sıfırladı. Önce klavyeyi yeniden okuyun.",
    noProfile: "En az bir profil seçin.",
    otherKeyboard: "Bu yedek başka bir klavyeye ait.",
    firmwareTick: "Bu yedeği karşılaştırmak için “Yine de geri yükle” kutusunu işaretleyin.",
  },
  dialog: {
    filter: "OpenWraith yedekleri",
    saveAll: "Tüm profillerin yedeğini kaydet",
    saveActive: "Etkin profilin yedeğini kaydet",
    open: "Geri yüklenecek yedeği seçin",
  },
  backup: {
    title: "Yedekle",
    intro: "Yedek dosyası klavyenin ayarlarını içerir: aktivasyon, Rapid Trigger, ışık, tuş atamaları ve daha fazlası.",
    all: "Dört profilin hepsini yedekle…",
    recommended: "Önerilen",
    allHint:
      "Okuma sırasında klavye profilleri tek tek dolaşır (ışık kısa süre değişir) ve bulunduğu profile geri döner. Hiçbir ayar değiştirilmez.",
    active: "Yalnızca etkin profili yedekle…",
    activeHint: (p) => `Profil değiştirmeden profil ${p.n} okunur.`,
    activeHintUnknown: "Profil değiştirmeden etkin profil okunur.",
    reviewTitle: "Dört profilin hepsi yedeklensin mi?",
    reviewSwitches: (p) =>
      `Klavye sırayla ${p.visits} numaralı profillere geçer (ışık kısa süre değişir), ardından ${p.original} numaralı profile döner. Hiçbir ayar değiştirilmez.`,
    reviewNoSwitches: "Yalnızca etkin profilin okunması gerekiyor; profil değiştirilmez.",
    reviewSaveWait: (p) => `OpenWraith önce klavyenin daha önceki bir değişikliği kaydetmesi için ${p.seconds} bekler.`,
    reviewStop: "İstediğiniz an durdurabilirsiniz; klavye o zaman bulunduğu profile geri döner.",
    reviewFile: "Dosya",
    confirm: "Tüm profilleri yedekle",
    saved: "Yedek kaydedildi",
    noFileSaved: "Yedek dosyası kaydedilmedi.",
    savedSummary: (p) => `${p.profiles} · firmware ${p.firmware}`,
    savedAt: (p) => `Oluşturulma: ${p.date}.`,
    file: "Dosya:",
    undecoded: (p) => `Klavyenin yanıtlarından ${p.n} tanesi çözümlenemedi; okunduğu gibi kaydedildi.`,
    foreignActivity:
      "Klavye okunurken başka bir program klavyeyle konuştu. Bütün wraith.software sekmelerini kapatın ve emin olmak için yeniden yedekleyin.",
  },
  profiles: {
    all: (p) => `Tüm profiller (${p.range})`,
    one: (p) => `Yalnızca profil ${p.n}`,
    some: (p) => `Profiller: ${p.list}`,
  },
  restore: {
    title: "Geri yükle",
    intro:
      "Bir yedeğin ayarlarını klavyeye geri getirin. OpenWraith önce karşılaştırır ve neyin değişeceğini gösterir; siz onaylamadan hiçbir şey yazılmaz.",
    step1: "Yedeği seçin",
    step2: "Profilleri seçin",
    step3: "Klavyeyle karşılaştırın",
    openFile: "Dosya aç…",
    openOther: "Başka bir dosya aç…",
    orSafety: "Aşağıda listelenen güvenlik yedeklerinden birini de geri yükleyebilirsiniz.",
    opening: "Yedek açılıyor…",
    kinds: {
      user: "Yedek dosyası",
      safety: "Güvenlik yedeği",
    },
    facts: {
      made: "Oluşturulma",
      keyboard: "Klavye",
      firmware: "Firmware",
      profiles: "Profiller",
      tool: "Oluşturan program",
      reason: "Neden",
    },
    simulator: "Geliştirici simülatörüyle oluşturulmuş: gerçek bir klavyenin ayarlarını içermez.",
    undecoded: (p) => `Bu yedek oluşturulurken yanıtlardan ${p.n} tanesi çözümlenemedi.`,
    otherKeyboardTitle: "Bu yedek başka bir klavyeye ait",
    otherKeyboardBody: (p) =>
      `Şu modelde oluşturulmuş: ${p.model}. OpenWraith bir yedeği yalnızca aynı modele geri yükler.`,
    notChecked: "Bağlı bir klavye yok; bu yüzden OpenWraith bu yedeğin klavyeye uyup uymadığını henüz denetlemedi.",
    notInBackup: "Bu yedekte yok",
    firmwareTitle: "Bu yedek farklı bir firmware sürümüyle alınmış",
    firmwareBody: (p) =>
      `Yedek: firmware ${p.backup}. Klavye: firmware ${p.keyboard}. Firmware güncellemesinden sonra geri yüklemek olağan bir durumdur, ama bazı ayarların biçimi sürümler arasında değişmiş olabilir.`,
    firmwareAnyway: "Yine de geri yükle",
    firmwareAnywayHint: "OpenWraith yine önce karşılaştırır ve bir şey yazmadan önce her değişikliği gösterir.",
    compareHint: "OpenWraith klavyeyi okur ve neyin değişeceğini gösterir. Siz onaylamadan hiçbir şey yazılmaz.",
    compare: "Klavyeyle karşılaştır…",
    consentTitle: "Karşılaştırmak için profil değiştirilsin mi?",
    consentBody: (p) =>
      p.count === 1
        ? `Karşılaştırma için ${p.list} numaralı profil okunur: klavye bu profile geçer ve ${p.active} numaralı profile geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`
        : `Karşılaştırma için ${p.list} numaralı profiller okunur: klavye bu profillere sırayla geçer ve ${p.active} numaralı profile geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`,
    consentBodyUnknown: (p) =>
      p.count === 1
        ? `Karşılaştırma için ${p.list} numaralı profil okunur: klavye bu profile geçer ve geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`
        : `Karşılaştırma için ${p.list} numaralı profiller okunur: klavye bu profillere sırayla geçer ve geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`,
    consentConfirm: "Profil değiştir ve karşılaştır",
  },
  review: {
    title: "Bu yedek geri yüklensin mi?",
    undoTitle: (p) =>
      p.count === 1
        ? `Geri al: profil ${p.list}, ${p.date} tarihindeki durumuna döndürülsün mü?`
        : `Geri al: ${p.list} numaralı profiller, ${p.date} tarihindeki durumlarına döndürülsün mü?`,
    undoNote: (p) =>
      p.count === 1
        ? `Bu işlem profilin tamamını o zamanki durumuna döndürür; profil ${p.list} üzerinde sonradan yapılan değişiklikler de geri alınır.`
        : `Bu işlem bu profilleri o zamanki durumlarına döndürür; ${p.list} numaralı profillerde sonradan yapılan değişiklikler de geri alınır.`,
    facts: {
      file: "Yedek",
      backupMade: "Yedeğin tarihi",
      keyboardRead: "Klavyenin okunduğu an",
      activeProfile: "Etkin profil",
    },
    madeWith: (p) => `${p.date} · firmware ${p.firmware}`,
    returnsTo: (p) => `Profil ${p.n}; klavye sonunda bu profile döner`,
    switched: (p) =>
      p.count === 1
        ? `Karşılaştırmak için klavye ${p.list} numaralı profile geçip geri döndü.`
        : `Karşılaştırmak için klavye ${p.list} numaralı profillere geçip geri döndü.`,
    profileSummary: (p) => (p.changes === 0 ? `Profil ${p.n}: değişiklik yok` : `Profil ${p.n}: ${p.changes} değişiklik`),
    profileSkipped: (p) => `${p.n} ayar geri yüklenmeyecek`,
    notCompared: "Bu profil klavyeyle karşılaştırılmadı.",
    willWrite: "Yazılacak",
    notRestored: "Geri yüklenmeyecek",
    alreadyMatches: (p) => `Zaten aynı (${p.n})`,
    notInBackup: (p) => `Yedekte yok (${p.n})`,
    triggers: (p) => `Aktivasyon ve Rapid Trigger: ${p.n} tuş`,
    columns: {
      key: "Tuş",
      now: "Şu an",
      backup: "Yedekte",
      setting: "Ayar",
    },
    macros: (p) => `Makrolar ${p.list}`,
    advancedKeys: (p) => `Gelişmiş tuşlar ${p.list}`,
    commitNote: "Bunun yazılması, klavyenin tuş atamalarını kaydetmesini de sağlar.",
    moreChanges: (p) => `${p.n} değişiklik daha gösterilmiyor.`,
    totals: (p) =>
      p.count === 1
        ? `Profil ${p.list} üzerinde ${p.n} değişiklik`
        : `${p.list} numaralı profillerde toplam ${p.n} değişiklik`,
    packets: (p) => `Toplam ${p.writes} tablo yazımı, ${p.packets} paket`,
    packetsProfile: (p) => `Profil ${p.n}: ${p.writes} tablo yazımı, ${p.packets} paket`,
    confirm: (p) => `${p.n} değişikliği geri yükle`,
    nothing: "Yazılacak bir şey yok: klavye bu yedekle zaten aynı.",
    nothingSkips: "Yedeğin şu bölümleri geri yüklenemez:",
    safetyNote: (p) =>
      p.count === 1
        ? `Önce profil ${p.list} için bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`
        : `Önce ${p.list} numaralı profiller için bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`,
  },
  changes: {
    rtOff: "RT kapalı",
    rt: (p) => `RT ${p.value}`,
    rtSeparate: (p) => `RT ${p.press} / ${p.release}`,
    unknownMode: (p) => `Bilinmeyen mod ${p.mode}`,
    trigger: (p) => `${p.actuation} · ${p.rt}`,
    raw: "Çözümlenmemiş değer",
    light: {
      effect: "Efekt",
      color: "Renk",
      brightness: "Parlaklık",
      speed: "Hız",
    },
    unknownEffect: (p) => `Bilinmeyen efekt (${p.id})`,
    keymapKeys: (p) => `${p.n} tuş farklı (${p.keys})`,
    deadBand: (p) => `üst ${p.top}, alt ${p.bottom}`,
    deadBandNone: "ayarlanmamış",
    colors: (p) => `${p.n} tuşun rengi farklı`,
    colorNone: "yok",
    macroEmpty: "boş",
    macroSteps: (p) => `${p.n} adım`,
    macroInUse: "kullanımda",
    advancedEmpty: "boş",
    advancedInUse: (p) => `kullanımda (tür ${p.kind})`,
    entries: (p) => `${p.n} kayıt farklı`,
  },
  result: {
    headline: {
      done: (p) =>
        p.n === 1
          ? "Geri yükleme tamamlandı. Değişiklik yazıldı, doğrulandı ve klavye onu sakladı (profil değiştirilip denetlendi)."
          : `Geri yükleme tamamlandı. ${p.n} değişikliğin hepsi yazıldı, doğrulandı ve klavye bunları sakladı (profil değiştirilip denetlendi).`,
      doneUnchecked: (p) =>
        p.n === 1
          ? "Geri yükleme tamamlandı. Değişiklik yazıldı ve doğrulandı."
          : `Geri yükleme tamamlandı. ${p.n} değişiklik yazıldı ve doğrulandı.`,
      nothingToDo: "Yazılacak bir şey yoktu: klavye bu yedekle zaten aynıydı.",
      cancelled: (p) => `Durduruldu. Durdurmadan önce ${p.written} değişiklik yazıldı; ${p.rest} değişiklik yazılmadı.`,
      notWritten: "Hiçbir şey yazılmadı.",
      failed: (p) =>
        `Geri yükleme bir hatayla durdu. ${p.total} değişiklikten ${p.written} tanesi yazıldı ve doğrulandı; geri kalanı yazılmadı.`,
      maybeLost: (p) =>
        `Yazıldı ve doğrulandı, ama klavye ${p.n} değişikliği kaydetmemiş olabilir. Denetlemek için klavyeyi yeniden okuyun ya da geri alın.`,
      lost: (p) => `Klavye ${p.n} değişikliği saklamadı. Geri yüklemeyi yeniden çalıştırın ya da geri alın.`,
    },
    persistenceTitle: "Kalıcılık denetimi",
    persistence: {
      profiles: (p) =>
        p.count === 1
          ? `Başka bir profile geçildikten sonra profil ${p.list} yeniden seçildi.`
          : `Başka bir profile geçildikten sonra ${p.list} numaralı profiller yeniden seçildi.`,
      kept: (p) => `${p.n} değişiklik profil değiştirildikten sonra da yerinde.`,
      lost: (p) => `${p.n} değişiklik profil değiştirildikten sonra kayboldu.`,
      notChecked: (p) => `Profiller arasında ortak olan ayarlardaki ${p.n} değişiklik ayrıca denetlenemedi.`,
    },
    finalTitle: "Son denetim",
    finalOk: (p) =>
      p.count === 1
        ? `Profil ${p.list} yeniden okundu; yedekteki değerleri taşıyor.`
        : `${p.list} numaralı profiller yeniden okundu; yedekteki değerleri taşıyorlar.`,
    finalMismatches: "Şu ayarlar artık yedekteki değerleri taşımıyor:",
    finalMismatch: (p) => `Profil ${p.n}: ${p.table}`,
    counts: {
      title: "Sayılarla",
      written: "Yazıldı ve doğrulandı",
      kept: "Profil değişiminden sonra korundu",
      maybeLost: "Kaydedilmemiş olabilir",
      lost: "Kayboldu",
      failed: "Başarısız",
      notAttempted: "Yazılmadı",
      alreadyMatched: "Zaten aynıydı",
      resent: "Değiştirilmeden yeniden gönderildi",
      skipped: "Geri yüklenmedi",
    },
    nextSteps: "Sonraki adımlar",
    runAgain: "Geri yüklemeyi yeniden çalıştır",
    connectFirst: "Önce klavyeyi bağlayın.",
  },
  safety: {
    title: "Güvenlik yedekleri",
    intro:
      "OpenWraith her değişiklikten önce otomatik olarak bir tane kaydeder. Birini geri yüklemek, o zamandan beri yapılan değişiklikleri geri alır.",
    empty: "Henüz güvenlik yedeği yok. OpenWraith her değişiklikten önce otomatik olarak bir tane oluşturur.",
    loadFailed: "Güvenlik yedeklerinin listesi okunamadı.",
    columns: {
      made: "Oluşturulma",
      reason: "Neden",
      keyboard: "Klavye",
      actions: "İşlemler",
    },
    restore: "Geri yükle…",
    restoreRow: (p) => `${p.date} tarihli güvenlik yedeğini geri yükle (${p.reason})`,
    showRow: (p) => `${p.date} tarihli güvenlik yedeğini klasöründe göster`,
    refresh: "Listeyi yenile",
    showFolder: "Klasörü göster",
    count: (p) => `${p.total} yedekten ${p.shown} tanesi gösteriliyor`,
  },
  skipReasons: {
    unconfirmed_command: "OpenWraith bu ayarı henüz yazmıyor: yazılması klavyede doğrulanmadı.",
    no_known_command: "OpenWraith bu ayarı nasıl yazacağını bilmiyor.",
    unsupported_light_mode: (p) => `Yedekte yazılamayan bir ışık efekti var (${p.mode}).`,
    invalid_light_record: "Yedekteki ışık ayarları geçersiz.",
    knob: "Döner düğmenin tetikleme ayarı yok.",
    unknown_trigger_mode: (p) => `Yedekte bilinmeyen bir tetikleme modu var (${p.mode}).`,
    second_travel_differs: "Tuş, OpenWraith'in değiştirmediği ikinci bir aktivasyon mesafesi kullanıyor.",
    press_release_differ: "Tuşun Rapid Trigger basma ve bırakma değerleri, aynı olmaları gereken bir modda farklı.",
    out_of_range: "Yedekteki bir değer klavyenin kabul ettiği aralığın dışında.",
    trigger_limits_unknown: "Klavyenin tetikleme sınırları bilinmiyor.",
    dead_band_out_of_range: "Yedekteki bir ölü bölge değeri klavyenin kabul ettiği aralığın dışında.",
    dead_band_unit_unknown: "Klavyenin ölü bölge birimi bilinmiyor.",
    distance_unit_changed: (p) => `Mesafe birimi farklı (yedek: ${p.backup}, klavye: ${p.keyboard}).`,
    keymap_group_blocked: (p) =>
      `Tuş atamaları, makrolar ve gelişmiş tuşlar ancak birlikte geri yüklenebilir ve burada bu güvenli değil: ${p.cause}`,
    not_in_keyboard: "Klavyede bu ayar yok.",
    unusable: "Yedeğin bu bölümü kullanılamıyor.",
  },
  blockCauses: {
    non_empty_advanced_key: (p) =>
      `yedekteki ${p.slot} numaralı gelişmiş tuş kullanımda ve gelişmiş tuşların yazılması klavyede henüz doğrulanmadı.`,
    non_canonical_macro: (p) => `yedekteki ${p.slot} numaralı makro alışılmadık bir biçimde.`,
    malformed_reference: (p) => `yedekteki ${p.layer} içinde bozuk bir kayıt var (${p.index} numaralı).`,
    incomplete: (p) => `şu bölüm tam okunamadı: ${p.table}.`,
    fn_keymap_not_writable: "Fn katmanı tuş atamaları yazılamıyor; bu yüzden klavye tuş atamalarını kaydetmez.",
  },
  layers: {
    base: "tuş atamaları",
    fn: "Fn katmanı tuş atamaları",
  },
  planWarnings: {
    firmware_differs: (p) => `Yedek ${p.backup} sürümlü firmware ile alınmış; klavyede ${p.keyboard} sürümü var.`,
    constant_differs: "Normalde hiç değişmeyen bir değer yedekle klavye arasında farklı.",
    capability_flags_differ: "Klavye, yedeğin alındığı zamandan farklı özellikler bildiriyor.",
    shared_tables_may_change_other_profiles: (p) =>
      `Şu ayarlar profiller arasında ortak olabilir: ${p.tables}. Bunları geri yüklemek şu profilleri de değiştirebilir: ${p.profiles}.`,
    likely_shared_table_differs: (p) =>
      `Profiller arasında büyük olasılıkla ortak olan bir ayar yedekten farklı: ${p.table}.`,
  },
  safetyReasons: {
    set_trigger: "Aktivasyon değişikliğinden önce",
    set_light: "Işık değişikliğinden önce",
    restore: "Geri yüklemeden önce",
    other: "Güvenlik yedeği",
  },
};
