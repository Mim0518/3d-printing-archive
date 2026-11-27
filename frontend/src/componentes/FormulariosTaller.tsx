import {
  Alert,
  Autocomplete,
  Button,
  ColorInput,
  Group,
  NumberInput,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Switch,
  Text,
  TextInput,
  Textarea,
} from '@mantine/core'
import { useMutation } from '@tanstack/react-query'
import { useState } from 'react'

import { api } from '../api'
import { aNumero, nombreMaterial, useRefrescarTaller, useTaller } from '../opciones'
import type { Campo, Impresora, Material, Parametros, Tecnologia } from '../tipos'

type Num = number | string

// ---------- impresora ----------

function inicialImpresora(i?: Partial<Impresora>) {
  return {
    nombre: i?.nombre ?? '',
    tecnologia: (i?.tecnologia ?? 'resina') as Tecnologia,
    marca: i?.marca ?? '',
    modelo: i?.modelo ?? '',
    volumen_x_mm: (i?.volumen_x_mm ?? '') as Num,
    volumen_y_mm: (i?.volumen_y_mm ?? '') as Num,
    volumen_z_mm: (i?.volumen_z_mm ?? '') as Num,
    boquilla_mm: (i?.boquilla_mm ?? '') as Num,
    res_x: (i?.res_x ?? '') as Num,
    res_y: (i?.res_y ?? '') as Num,
    pixel_um: (i?.pixel_um ?? '') as Num,
    potencia_w: (i?.potencia_w ?? '') as Num,
    carpeta: i?.carpeta ?? '',
    en_uso: i?.en_uso ?? true,
    notas: i?.notas ?? '',
  }
}

/** Alta o edición de una impresora. `inicial` sin id es una nueva (puede venir prellenada). */
export function FormImpresora({
  inicial,
  onListo,
}: {
  inicial?: Partial<Impresora>
  onListo?: (id: number) => void
}) {
  const refrescar = useRefrescarTaller()
  const nueva = !inicial?.id
  const [f, setF] = useState(() => inicialImpresora(inicial))
  const poner = (cambios: Partial<typeof f>) => setF({ ...f, ...cambios })
  const guardar = useMutation({
    mutationFn: () => {
      const cuerpo = {
        ...f,
        volumen_x_mm: aNumero(f.volumen_x_mm),
        volumen_y_mm: aNumero(f.volumen_y_mm),
        volumen_z_mm: aNumero(f.volumen_z_mm),
        boquilla_mm: f.tecnologia === 'fdm' ? aNumero(f.boquilla_mm) : null,
        res_x: f.tecnologia === 'resina' ? aNumero(f.res_x) : null,
        res_y: f.tecnologia === 'resina' ? aNumero(f.res_y) : null,
        pixel_um: f.tecnologia === 'resina' ? aNumero(f.pixel_um) : null,
        potencia_w: aNumero(f.potencia_w),
      }
      return nueva
        ? api<{ id: number }>('/impresoras', { method: 'POST', body: JSON.stringify(cuerpo) })
        : api<{ ok: boolean }>(`/impresoras/${inicial.id}`, { method: 'PATCH', body: JSON.stringify(cuerpo) }).then(() => ({
            id: inicial.id!,
          }))
    },
    onSuccess: (r) => {
      refrescar()
      onListo?.(r.id)
    },
  })
  const conPerfiles = (inicial?.n_perfiles ?? 0) > 0
  return (
    <Stack gap="sm">
      <TextInput label="Nombre" required value={f.nombre} onChange={(e) => poner({ nombre: e.currentTarget.value })} />
      <div>
        <Text size="sm" fw={500} mb={4}>
          Tecnología
        </Text>
        <SegmentedControl
          size="xs"
          value={f.tecnologia}
          disabled={conPerfiles}
          onChange={(v) => poner({ tecnologia: v as Tecnologia })}
          data={[
            { value: 'resina', label: 'Resina' },
            { value: 'fdm', label: 'FDM (filamento)' },
          ]}
        />
        {conPerfiles && (
          <Text size="xs" c="dimmed">
            Tiene perfiles, así que su tecnología ya no cambia.
          </Text>
        )}
      </div>
      <SimpleGrid cols={{ base: 1, xs: 2 }}>
        <TextInput label="Marca" value={f.marca} onChange={(e) => poner({ marca: e.currentTarget.value })} />
        <TextInput label="Modelo" value={f.modelo} onChange={(e) => poner({ modelo: e.currentTarget.value })} />
      </SimpleGrid>
      <SimpleGrid cols={3}>
        <NumberInput label="Ancho (X)" suffix=" mm" min={0} value={f.volumen_x_mm} onChange={(v) => poner({ volumen_x_mm: v })} />
        <NumberInput label="Fondo (Y)" suffix=" mm" min={0} value={f.volumen_y_mm} onChange={(v) => poner({ volumen_y_mm: v })} />
        <NumberInput label="Alto (Z)" suffix=" mm" min={0} value={f.volumen_z_mm} onChange={(v) => poner({ volumen_z_mm: v })} />
      </SimpleGrid>
      {f.tecnologia === 'resina' ? (
        <>
          <SimpleGrid cols={3}>
            <NumberInput label="Pantalla: pixeles a lo ancho" min={1} allowDecimal={false} value={f.res_x} onChange={(v) => poner({ res_x: v })} />
            <NumberInput label="a lo largo" min={1} allowDecimal={false} value={f.res_y} onChange={(v) => poner({ res_y: v })} />
            <NumberInput label="Pixel" suffix=" µm" min={0} value={f.pixel_um} onChange={(v) => poner({ pixel_um: v })} />
          </SimpleGrid>
          <Text size="xs" c="dimmed">
            Con la resolución de la pantalla la app reconoce qué laminados son de esta impresora.
          </Text>
        </>
      ) : (
        <NumberInput
          label="Boquilla"
          suffix=" mm"
          min={0.1}
          max={2}
          step={0.1}
          decimalScale={2}
          value={f.boquilla_mm}
          onChange={(v) => poner({ boquilla_mm: v })}
          maw={200}
        />
      )}
      <NumberInput
        label="Potencia promedio al imprimir"
        description="Para el costo de la luz (fase 6). Opcional."
        suffix=" W"
        min={1}
        value={f.potencia_w}
        onChange={(v) => poner({ potencia_w: v })}
        maw={300}
      />
      <TextInput
        label="Carpeta de documentos"
        description="Dentro de la colección, por ejemplo Printers/SLA/Photon Mono 4: sus manuales, mejoras y calibraciones."
        value={f.carpeta}
        onChange={(e) => poner({ carpeta: e.currentTarget.value })}
      />
      <Switch label="La sigo usando" checked={f.en_uso} onChange={(e) => poner({ en_uso: e.currentTarget.checked })} />
      <Textarea label="Notas" autosize minRows={2} value={f.notas} onChange={(e) => poner({ notas: e.currentTarget.value })} />
      {guardar.error && <Alert color="red">{guardar.error.message}</Alert>}
      <Group>
        <Button loading={guardar.isPending} disabled={!f.nombre.trim()} onClick={() => guardar.mutate()}>
          {nueva ? 'Agregar impresora' : 'Guardar'}
        </Button>
      </Group>
    </Stack>
  )
}

// ---------- material ----------

const SUBTIPOS = {
  resina: ['Estándar', 'Tipo ABS', 'Lavable con agua', 'De origen vegetal', 'Flexible', 'Alta definición'],
  filamento: ['PLA', 'PLA+', 'PLA mate', 'PLA seda', 'PETG', 'ABS', 'ASA', 'TPU'],
}

function inicialMaterial(m?: Partial<Material>) {
  return {
    tipo: (m?.tipo ?? 'resina') as Material['tipo'],
    marca: m?.marca ?? '',
    nombre: m?.nombre ?? '',
    subtipo: m?.subtipo ?? '',
    color: m?.color ?? '',
    color_hex: m?.color_hex ?? '',
    presentacion: (m?.presentacion ?? 1000) as Num,
    precio: (m?.precio ?? '') as Num,
    densidad: (m?.densidad ?? '') as Num,
    diametro_mm: (m?.diametro_mm ?? (m?.tipo === 'filamento' ? 1.75 : '')) as Num,
    queda: (m?.queda ?? '') as Num,
    en_uso: m?.en_uso ?? true,
    notas: m?.notas ?? '',
  }
}

export function FormMaterial({ inicial, onListo }: { inicial?: Partial<Material>; onListo?: () => void }) {
  const refrescar = useRefrescarTaller()
  const nuevo = !inicial?.id
  const [f, setF] = useState(() => inicialMaterial(inicial))
  const poner = (cambios: Partial<typeof f>) => setF({ ...f, ...cambios })
  const unidad = f.tipo === 'resina' ? 'ml' : 'g'
  const guardar = useMutation({
    mutationFn: () => {
      const cuerpo = {
        ...f,
        presentacion: aNumero(f.presentacion),
        precio: aNumero(f.precio),
        densidad: aNumero(f.densidad),
        diametro_mm: f.tipo === 'filamento' ? aNumero(f.diametro_mm) : null,
        queda: aNumero(f.queda),
      }
      return nuevo
        ? api('/materiales', { method: 'POST', body: JSON.stringify(cuerpo) })
        : api(`/materiales/${inicial.id}`, { method: 'PATCH', body: JSON.stringify(cuerpo) })
    },
    onSuccess: () => {
      refrescar()
      onListo?.()
    },
  })
  const borrar = useMutation({
    mutationFn: () => api(`/materiales/${inicial?.id}`, { method: 'DELETE' }),
    onSuccess: () => {
      refrescar()
      onListo?.()
    },
  })
  const enUso = (inicial?.n_perfiles ?? 0) + (inicial?.n_placas ?? 0) > 0
  return (
    <Stack gap="sm">
      <SegmentedControl
        size="xs"
        value={f.tipo}
        disabled={enUso}
        onChange={(v) => poner({ tipo: v as Material['tipo'] })}
        data={[
          { value: 'resina', label: 'Resina' },
          { value: 'filamento', label: 'Filamento' },
        ]}
      />
      <SimpleGrid cols={{ base: 1, xs: 2 }}>
        <TextInput label="Marca" value={f.marca} onChange={(e) => poner({ marca: e.currentTarget.value })} />
        <TextInput label="Nombre" required value={f.nombre} onChange={(e) => poner({ nombre: e.currentTarget.value })} />
        <Autocomplete label="Tipo" data={SUBTIPOS[f.tipo]} value={f.subtipo} onChange={(v) => poner({ subtipo: v })} />
        <Group gap="xs" grow>
          <TextInput label="Color" value={f.color} onChange={(e) => poner({ color: e.currentTarget.value })} />
          <ColorInput label="Muestra" value={f.color_hex} onChange={(v) => poner({ color_hex: v })} format="hex" />
        </Group>
      </SimpleGrid>
      <SimpleGrid cols={{ base: 2, xs: 3 }}>
        <NumberInput label="Presentación" suffix={` ${unidad}`} min={0} value={f.presentacion} onChange={(v) => poner({ presentacion: v })} />
        <NumberInput label="Precio" prefix="$" min={0} decimalScale={2} value={f.precio} onChange={(v) => poner({ precio: v })} />
        <NumberInput label="Densidad" suffix=" g/cm³" min={0.3} max={5} decimalScale={3} value={f.densidad} onChange={(v) => poner({ densidad: v })} />
        {f.tipo === 'filamento' && (
          <NumberInput label="Diámetro" suffix=" mm" min={0.5} max={5} decimalScale={2} value={f.diametro_mm} onChange={(v) => poner({ diametro_mm: v })} />
        )}
        <NumberInput label="Queda (opcional)" suffix={` ${unidad}`} min={0} value={f.queda} onChange={(v) => poner({ queda: v })} />
      </SimpleGrid>
      <Switch label="Lo sigo usando" checked={f.en_uso} onChange={(e) => poner({ en_uso: e.currentTarget.checked })} />
      <Textarea label="Notas" autosize minRows={2} value={f.notas} onChange={(e) => poner({ notas: e.currentTarget.value })} />
      {(guardar.error ?? borrar.error) && <Alert color="red">{(guardar.error ?? borrar.error)?.message}</Alert>}
      <Group justify="space-between">
        <Button loading={guardar.isPending} disabled={!f.nombre.trim()} onClick={() => guardar.mutate()}>
          {nuevo ? 'Agregar material' : 'Guardar'}
        </Button>
        {!nuevo && (
          <Button
            variant="subtle"
            color="red"
            loading={borrar.isPending}
            disabled={enUso}
            title={enUso ? 'Lo usan perfiles o placas: márcalo como que ya no lo usas' : undefined}
            onClick={() => borrar.mutate()}
          >
            Borrar
          </Button>
        )}
      </Group>
    </Stack>
  )
}

// ---------- perfil ----------

/** Los valores de un perfil según los campos de su tecnología. */
export function CamposPerfil({
  campos,
  tecnologia,
  valores,
  onChange,
}: {
  campos: Campo[]
  tecnologia: Tecnologia
  valores: Parametros
  onChange: (v: Parametros) => void
}) {
  const suyos = campos.filter((c) => c.tecnologia === tecnologia)
  const poner = (clave: string, v: number | string | null) => {
    const x = { ...valores }
    if (v === null || v === '') delete x[clave]
    else x[clave] = v
    onChange(x)
  }
  return (
    <SimpleGrid cols={{ base: 1, xs: 2, md: 3 }}>
      {suyos.map((c) =>
        c.tipo === 'texto' ? (
          <TextInput
            key={c.clave}
            label={c.etiqueta}
            value={(valores[c.clave] as string) ?? ''}
            onChange={(e) => poner(c.clave, e.currentTarget.value)}
          />
        ) : (
          <NumberInput
            key={c.clave}
            label={c.etiqueta}
            suffix={c.unidad ? ` ${c.unidad}` : undefined}
            min={c.min}
            max={c.max}
            allowDecimal={c.tipo === 'numero'}
            decimalScale={c.tipo === 'numero' ? 4 : 0}
            value={(valores[c.clave] as number) ?? ''}
            onChange={(v) => poner(c.clave, aNumero(v))}
          />
        ),
      )}
    </SimpleGrid>
  )
}

/** Alta rápida de un perfil: nombre, impresora y material. Los valores se capturan en su página. */
export function FormPerfilNuevo({ impresoraId, onListo }: { impresoraId?: number; onListo: (id: number) => void }) {
  const { impresoras, materiales } = useTaller()
  const refrescar = useRefrescarTaller()
  const [nombre, setNombre] = useState('')
  const [imp, setImp] = useState<string | null>(impresoraId ? String(impresoraId) : null)
  const [mat, setMat] = useState<string | null>(null)
  const tecnologia = impresoras.find((i) => String(i.id) === imp)?.tecnologia
  const tipo = tecnologia === 'fdm' ? 'filamento' : 'resina'
  const crear = useMutation({
    mutationFn: () =>
      api<{ id: number }>('/perfiles', {
        method: 'POST',
        body: JSON.stringify({ nombre, impresora_id: Number(imp), material_id: mat ? Number(mat) : null }),
      }),
    onSuccess: (r) => {
      refrescar()
      onListo(r.id)
    },
  })
  return (
    <Stack gap="sm">
      <TextInput label="Nombre" placeholder="Por ejemplo: Sunlu gris 0.03 mm" value={nombre} onChange={(e) => setNombre(e.currentTarget.value)} />
      <Select
        label="Impresora"
        data={impresoras.map((i) => ({ value: String(i.id), label: i.nombre }))}
        value={imp}
        onChange={(v) => {
          setImp(v)
          setMat(null)
        }}
      />
      <Select
        label="Material"
        placeholder="Cualquiera"
        clearable
        disabled={!imp}
        data={materiales.filter((m) => m.tipo === tipo).map((m) => ({ value: String(m.id), label: nombreMaterial(m) }))}
        value={mat}
        onChange={setMat}
      />
      {crear.error && <Alert color="red">{crear.error.message}</Alert>}
      <Button disabled={!nombre.trim() || !imp} loading={crear.isPending} onClick={() => crear.mutate()}>
        Crear y capturar sus valores
      </Button>
    </Stack>
  )
}
