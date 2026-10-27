// Yazma akışı: adımlar, ilerleme, onay alt bilgisi, sonuçlar, tablo adları,
// yazma uyarıları ve kaydetme tetikleyicisi gerekçeleri.

import type { flow as en } from "../en/flow";
import type { Messages } from "../types";

export const flow: Messages<typeof en> = {
  opKinds: {
    refresh: "Klavye okunuyor",
    connect: "Bağlanılıyor",
    profile_switch: "Profil değiştiriliyor",
    trigger_edit: "Aktivasyon değiştiriliyor",
    light_edit: "Işık değiştiriliyor",
    backup_active: "Etkin profil yedekleniyor",
    backup_all: "Tüm profiller yedekleniyor",
    restore_prepare: "Yedekle karşılaştırılıyor",
    restore: "Yedek geri yükleniyor",
    recheck: "Yeniden denetleniyor",
  },
  titles: {
    refresh: "Klavye okunuyor",
    connect: "Bağlanılıyor",
    profile_switch: "Profil değişimi",
    trigger_edit: "Aktivasyon değişikliği",
    light_edit: "Işık değişikliği",
    backup_active: "Etkin profilin yedeği",
    backup_all: "Tüm profillerin yedeği",
    restore_prepare: "Geri yükleme",
    restore: "Geri yükleme",
    recheck: "Yeniden denetleniyor",
  },
  preparing: "Klavyedeki güncel ayarlar okunuyor…",
  steps: {
    checking_other_programs: "Klavyeyi başka bir programın kullanmadığı denetleniyor",
    reading: {
      snapshot: "Klavyenin ayarları okunuyor",
      safety_backup: "Güvenlik yedeği için geçerli ayarlar okunuyor",
      backup: "Klavye okunuyor",
      compare: "Karşılaştırmak için klavye okunuyor",
    },
    readingProfile: {
      snapshot: (p) => `Profil ${p.n} okunuyor`,
      safety_backup: (p) => `Güvenlik yedeği için profil ${p.n} okunuyor`,
      backup: (p) => `Profil ${p.n} okunuyor`,
      compare: (p) => `Karşılaştırmak için profil ${p.n} okunuyor`,
    },
    selecting_profile: (p) => `${p.n} numaralı profile geçiliyor`,
    returning_to_profile: (p) => `${p.n} numaralı profile dönülüyor`,
    comparing: "Yedekle karşılaştırılıyor",
    saving_safety_backup: "Güvenlik yedeği kaydediliyor",
    saving_file: "Yedek dosyası kaydediliyor",
    writing: (p) => `Profil ${p.n} yazılıyor`,
    waiting_for_keyboard_save: "Klavyenin kaydetmesi bekleniyor",
    checking_persistence: (p) => `Profil ${p.n} değişiklikleri sakladı mı, denetleniyor`,
    final_check: "Son denetim",
    refreshing_view: "Yeni değerler okunuyor",
  },
  progress: {
    count: (p) => `${p.done}/${p.total}`,
    item: (p) => `${p.what} · ${p.item}/${p.items}`,
    packet: (p) => `Paket ${p.packet}/${p.packets}`,
    resend: "Yeniden gönderiliyor: ilk geri okuma farklı çıktı.",
    keepPlugged:
      "Bu işlem bitene kadar klavyeyi çıkarmayın ve klavyede profil değiştirmeyin.",
    checkingOthers: (p) => `Yazmadan önce klavye ${p.seconds} daha izleniyor.`,
    waitProfile: (p) => `Klavyenin profil ${p.n} ayarlarını kaydetmesi bekleniyor…`,
    waitGeneric: "Klavyenin kaydetmesi bekleniyor…",
    waitExplain:
      "Klavye bir değişikliği aldıktan yaklaşık 1 saniye sonra kaydeder; OpenWraith emin olmak için 2 saniye bekler.",
    waitLeft: (p) => `Kalan: ${p.seconds}`,
    steps: "Adımlar",
    done: "Tamamlandı",
    current: "Sürüyor",
  },
  cancel: {
    available: "Durdur",
    requested: "Geçerli adımdan sonra durdurulacak…",
    finishing_keymap_group: "Klavyenin saklayabilmesi için önce tuş atamalarının yazımı tamamlanıyor…",
    not_now: "Bu adım durdurulamaz; birazdan biter.",
    not_cancellable: "Bu işlem başladıktan sonra durdurulamaz.",
  },
  review: {
    safetyNote: (p) => `Önce profil ${p.n} için bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`,
    safetyNoteGeneric: (p) => `Önce bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`,
    vendorNote:
      "Bütün wraith.software sekmelerinin kapalı olduğundan emin olun. Site arka plandaki bir sekmeden bile klavyeyle konuşmaya devam eder.",
    vendorTick: "Bütün wraith.software sekmeleri kapalı",
    vendorTickNeeded: "wraith.software sekmelerinin kapalı olduğunu onaylayın.",
    firmwareNote: (p) => `${p.firmware} sürümlü firmware henüz OpenWraith ile test edilmedi.`,
    saveNote:
      "Yazma birkaç saniye sürer; ardından OpenWraith klavyenin kaydetmesi için yaklaşık 2 saniye bekler.",
    blocked: "Şu anda yazılamıyor:",
    invalidated: "Bu plan artık güncel değil:",
    arming: "Yaz düğmesi birazdan etkinleşecek.",
  },
  nothingWritten: "Hiçbir şey yazılmadı.",
  nothingWrittenSwitches: (p) =>
    `Hiçbir şey yazılmadı (profil geçişleri dışında; klavye şu an profil ${p.n} üzerinde).`,
  nothingWrittenSwitchesUnknown: "Hiçbir şey yazılmadı (profil geçişleri dışında).",
  results: {
    persisted: "Yazıldı ve doğrulandı · profil değişiminden sonra da korundu",
    written: "Yazıldı ve doğrulandı",
    resentUnchanged: "Tuş atamaları saklansın diye yeniden gönderildi",
    alreadyMatched: "Zaten aynıydı (yazılmadı)",
    maybeLost: "Yazıldı ve doğrulandı, ama kaydedilmemiş olabilir",
    maybeLostBaseKeymap: "Bir sonraki profil değişiminde ya da güç kesilince kaybolacak",
    lost: "KAYBOLDU: klavye saklamadı",
    failed: (p) => `Başarısız: ${p.reason}`,
    notAttempted: "Yazılmadı",
  },
  headline: {
    doneKeys: (p) =>
      `Tamamlandı. ${p.n} tuş yazıldı ve doğrulandı. Klavyenin kaydetmesi için yeterince beklendi.`,
    doneLight: "Işık yazıldı ve doğrulandı. Klavyenin kaydetmesi için yeterince beklendi.",
    done: "Tamamlandı. Her şey yazıldı ve doğrulandı. Klavyenin kaydetmesi için yeterince beklendi.",
    nothingToDo: "Değiştirilecek bir şey yok: klavyede bu ayarlar zaten var.",
    cancelled: (p) =>
      `Durduruldu. Durdurulmadan önce ${p.total} öğeden ${p.written} tanesi yazıldı; kalanlar yazılmadı.`,
    notWritten: "Hiçbir şey yazılmadı.",
    failed: (p) =>
      `Yazma bir hatayla durdu. ${p.total} öğeden ${p.written} tanesi yazılıp doğrulandı; kalanlar yazılmadı.`,
    maybeLost: (p) =>
      `Yazıldı ve doğrulandı, ama klavye bunlardan ${p.n} tanesini kaydetmemiş olabilir. Denetlemek için klavyeyi yeniden okuyun ya da geri alın.`,
    maybeLostOtherProfile: (p) =>
      `Yazıldı ve doğrulandı, ama klavye kaydetmeden önce başka bir profile geçtiği için bunlardan ${p.n} tanesini kaydetmemiş olabilir. Değişikliğiniz profil ${p.profile} içindi. Denetlemek için ${p.profile} numaralı profile geri dönüp klavyeyi yeniden okuyun ya da geri alın.`,
    lost: (p) =>
      `Klavye ${p.n} değişikliği saklamadı. Yeniden yazın ya da geri alın.`,
    waited: (p) => `OpenWraith klavyenin kaydetmesi için ${p.seconds} bekledi.`,
  },
  tables: {
    profile: "Profil ayarları",
    main_light: "Ana ışık",
    side_light: "Yan ışık",
    active_custom_table: "Etkin tuş rengi tablosu",
    custom_colors: (p) => `Tuş renkleri ${p.n}`,
    keymap: { base: "Tuş atamaları", fn: "Fn katmanı tuş atamaları" },
    macro: (p) => `Makro ${p.n}`,
    advanced_key: (p) => `Gelişmiş tuş ${p.n}`,
    triggers: "Aktivasyon ve Rapid Trigger",
    dead_band: "Ölü bölge",
    socd_pairs: "SOCD çiftleri",
    socd_settings: "SOCD ayarları",
    socd: "SOCD",
    switch_types: "Switch türleri",
    poll_rate: "Yoklama hızı",
    win_lock_and_rollover: "Win kilidi ve çoklu tuş algılama",
  },
  warnings: {
    ack_timeout: (p) =>
      `${p.of} paketten ${p.packet}. paketin onayı zamanında gelmedi (${p.send}. deneme).`,
    optional_ack_missing: (p) => `${p.packet}. paket için isteğe bağlı onay gelmedi.`,
    verify_reread: "Geri okuma için ikinci bir okuma gerekti.",
    retried: "İlk geri okuma farklı çıktığı için ikinci kez gönderildi.",
    select_ack_missing: (p) => `Klavye ${p.profile} numaralı profile geçişi onaylamadı.`,
    profile_push_missing: (p) => `Klavye geçişten sonra profil ${p.profile} bildirimini göndermedi.`,
    select_retried: (p) => `${p.profile} numaralı profile geçiş iki kez gönderildi.`,
    waited_for_late_ack: "OpenWraith geciken bir onayı bekledi.",
    waited_for_commit: (p) =>
      `OpenWraith önce klavyenin önceki bir değişikliği kaydetmesi için ${p.seconds} bekledi.`,
    base_keymap_not_saved:
      "Tuş atamaları yazıldı ama klavyenin onları kaydetmesini sağlayan adım tamamlanmadı.",
    unexpected_ack: (p) => `Klavye ${p.packet}. paketi onayladı; bunu genellikle yapmaz.`,
    socd_toggled_on_keyboard: (p) =>
      `Yazma sırasında ya da hemen ardından SOCD klavyeden değiştirildi (Fn + Sağ Shift); bu değişiklik üzerine yazılmış olabilir. SOCD şu an ${p.on ? "açık" : "kapalı"}.`,
  },
  commitTrigger: {
    base_keymap: "Fn katmanı tuş atamaları: klavye tuş atamalarını saklasın diye değiştirilmeden yeniden gönderilir",
    macros_or_advanced_keys: "Fn katmanı tuş atamaları: önlem olarak değiştirilmeden yeniden gönderilir (test edilmedi)",
  },
  footer: {
    safetyBackup: "Güvenlik yedeği:",
    undoDisabled: "Geri almak için klavyeyi bağlayın.",
    activeProfile: (p) => `Klavye profil ${p.n} üzerinde.`,
    activeProfileWhenLost: (p) => `Bağlantı kesildiğinde klavye profil ${p.n} üzerindeydi.`,
    activeProfileUnknown: "Etkin profil şu anda bilinmiyor.",
    editedProfile: (p) => `Bu değişiklik profil ${p.n} içindi.`,
    foreignActivity:
      "Bu işlem sırasında klavyeyle başka bir program konuştu. Bütün wraith.software sekmelerini kapatın.",
  },
  items: {
    title: "Ayrıntılar",
    sends: (p) => (p.n === 1 ? "Bir kez gönderildi" : `${p.n} kez gönderildi`),
    offsets: (p) => `Farklı baytlar: ${p.list}`,
    expected: "Beklenen",
    got: "Geri okunan",
  },
};
