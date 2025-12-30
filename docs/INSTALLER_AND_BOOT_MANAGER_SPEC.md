# RayOS Installer + Boot Manager Spec

Status: **draft / tracking stub**

Purpose: specify how RayOS installs from a USB and how boot selection works (Windows Boot Manager / GRUB-like).

---

## 1) Installer Wizard (USB Boot)

Minimum v0 flow:

1) Boot from USB (UEFI)
2) Installer wizard starts (text UI acceptable)
3) Select/create install partition for RayOS
4) Select RayOS data partition (or create)
5) Optional: choose VM storage locations (Linux/Windows) (partition/path)
6) Install RayOS + configure boot entries
7) Reboot into installed RayOS

---

## 2) Boot Manager Requirement

We need a boot manager experience similar to Windows Boot Manager / GRUB.

Options to decide:

- Use an existing UEFI boot manager (preferred if feasible)
- Build a minimal RayOS boot manager

Minimum required behaviors:

- Enumerate RayOS installs (and recovery entry)
- Default selection + timeout
- Clear logging of selection and failure reasons
- Recovery mode entry (boot installer/rescue)

---

## 3) Failure Modes to Document

- Install target mistakes (safety confirmations)
- ESP missing/readonly
- Secure Boot enabled (expected UX)
- Boot entry missing/corrupt

---

## 4) Related

- INSTALLABLE_RAYOS_PLAN.md
- BOOT_TROUBLESHOOTING.md
