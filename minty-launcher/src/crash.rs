use anyhow::{Context, Result};
use std::fs;

pub fn analyze_latest() -> Result<String> {
    let path = crate::config::data_dir()?.join("logs").join("minecraft.log");
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Не удалось прочитать журнал Minecraft: {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok("Журнал Minecraft пуст. Сначала запусти игру, чтобы получить данные для анализа.".to_owned());
    }
    let lines: Vec<&str> = content.lines().collect();
    let tail = lines.iter().rev().take(1200).copied().collect::<Vec<_>>();
    let joined = tail.iter().rev().copied().collect::<Vec<_>>().join("\n");
    let lower = joined.to_ascii_lowercase();
    let cause = tail.iter().find(|line| line.to_ascii_lowercase().contains("caused by:"))
        .copied().unwrap_or("Строка «Caused by» не найдена.");
    let diagnosis = if lower.contains("outofmemoryerror") || lower.contains("java heap space") {
        "Вероятная причина: не хватило Java-памяти. Уменьши число тяжёлых модов и проверь свободную ОЗУ."
    } else if lower.contains("mixin apply failed") || lower.contains("mixinapplyerror") {
        "Вероятная причина: несовместимый или устаревший мод/Mixin. Сверь версии модов с Minecraft и загрузчиком."
    } else if lower.contains("modresolutionexception") || lower.contains("could not resolve") {
        "Вероятная причина: не разрешились зависимости модов. Проверь обязательные библиотеки и совместимость сборки."
    } else if lower.contains("noclassdeffounderror") || lower.contains("classnotfoundexception") {
        "Вероятная причина: отсутствует класс или библиотека. Проверь зависимости и целостность файлов модов."
    } else if lower.contains("unsupportedclassversionerror") {
        "Вероятная причина: Java слишком старая для одного из файлов. Проверь требуемую версию Java."
    } else if lower.contains("glfw") || lower.contains("opengl") || lower.contains("lwjgl") {
        "Вероятная причина: графический драйвер или OpenGL. Обнови драйвер видеокарты и проверь графические моды."
    } else {
        "Однозначная причина не определена. Изучи первичную ошибку и строки стека ниже."
    };
    let relevant = tail.iter().find(|line| {
        let value = line.to_ascii_lowercase();
        value.contains("error") || value.contains("exception") || value.contains("failed")
    }).copied().unwrap_or("Строка ошибки не найдена.");
    let excerpt = tail.iter().take(10).rev().copied().collect::<Vec<_>>().join("\n");
    Ok(format!("{}\n\nПервая найденная ошибка:\n{}\n\nСтрока причины:\n{}\n\nПоследние записи:\n{}", diagnosis, relevant, cause, excerpt))
}