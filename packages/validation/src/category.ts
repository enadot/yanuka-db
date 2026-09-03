import { z } from 'zod';
import {
  CATEGORY_FIELD_OPERATORS,
  CATEGORY_RULE_FIELDS,
  CATEGORY_RULE_OPERATORS,
  RELATIONSHIP_TYPES,
  VALUELESS_OPERATORS,
} from '@yanuka/types';
import { CountryCodeSchema, optionalText, requiredText, UlidSchema } from './common.js';

const MAX_CONDITIONS = 12;
const MAX_VALUES = 20;
const MAX_DAYS = 3650;

export const CategoryConditionSchema = z
  .object({
    field: z.enum(CATEGORY_RULE_FIELDS),
    op: z.enum(CATEGORY_RULE_OPERATORS),
    values: z.array(z.string().trim().max(200, 'עד 200 תווים')).max(MAX_VALUES).default([]),
  })
  .superRefine((condition, context) => {
    if (!CATEGORY_FIELD_OPERATORS[condition.field].includes(condition.op)) {
      context.addIssue({ code: 'custom', message: 'התנאי אינו מתאים לשדה', path: ['op'] });
      return;
    }

    const values = condition.values.filter((value) => value.length > 0);
    if (VALUELESS_OPERATORS.includes(condition.op)) {
      if (values.length > 0) {
        context.addIssue({ code: 'custom', message: 'לתנאי זה אין ערכים', path: ['values'] });
      }
      return;
    }
    if (values.length === 0) {
      context.addIssue({ code: 'custom', message: 'יש להזין לפחות ערך אחד', path: ['values'] });
      return;
    }

    switch (condition.op) {
      case 'within_days': {
        const days = Number(values[0]);
        if (values.length !== 1 || !Number.isInteger(days) || days < 1 || days > MAX_DAYS) {
          context.addIssue({
            code: 'custom',
            message: `מספר ימים בין 1 ל־${MAX_DAYS}`,
            path: ['values'],
          });
        }
        break;
      }
      case 'similar':
        if (values.length !== 1 || values[0]!.length < 3) {
          context.addIssue({ code: 'custom', message: 'יש לכתוב משפט אחד', path: ['values'] });
        }
        break;
      default:
        break;
    }

    if (condition.field === 'country') {
      for (const value of values) {
        if (!CountryCodeSchema.safeParse(value).success) {
          context.addIssue({ code: 'custom', message: 'קוד מדינה אינו תקין', path: ['values'] });
          return;
        }
      }
    }
    if (condition.field === 'relationship') {
      for (const value of values) {
        if (!(RELATIONSHIP_TYPES as readonly string[]).includes(value)) {
          context.addIssue({ code: 'custom', message: 'סוג קשר אינו מוכר', path: ['values'] });
          return;
        }
      }
    }
  })
  .transform((condition) => ({
    ...condition,
    values: condition.values.filter((value) => value.length > 0),
  }));

export const CategoryRuleSchema = z.object({
  match: z.enum(['all', 'any']).default('all'),
  conditions: z
    .array(CategoryConditionSchema)
    .min(1, 'כלל צריך לפחות תנאי אחד')
    .max(MAX_CONDITIONS, `עד ${MAX_CONDITIONS} תנאים`),
});

export const CategoryInputSchema = z.object({
  name: requiredText(60, 'יש להזין שם קטגוריה'),
  description: optionalText(300),
  parentId: UlidSchema.nullable().default(null),
  icon: z
    .string()
    .trim()
    .regex(/^[a-z0-9-]{1,40}$/, 'אייקון אינו מוכר')
    .nullable()
    .default(null),
  color: z
    .string()
    .trim()
    .regex(/^#[0-9a-fA-F]{6}$/, 'צבע אינו תקין')
    .nullable()
    .default(null),
  rule: CategoryRuleSchema.nullable().default(null),
  showOnHome: z.boolean().default(true),
});

export const CategoryMembershipModeSchema = z.enum(['include', 'exclude', 'auto']);
